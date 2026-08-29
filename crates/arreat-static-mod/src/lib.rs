//! A bounded, local-only D2R loose-mod builder.

use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use arreat_data::{STATIC_MOD_TARGET_PATH, with_archive_reader};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MOD_DIRECTORY: &str = "arreat-index";
const ACTIVE_DIRECTORY: &str = "arreat-index.mpq";
const PREVIOUS_DIRECTORY: &str = "arreat-index.mpq.previous";
const MANIFEST_PATH: &str = "arreat-index-build.json";
const MODINFO_PATH: &str = "modinfo.json";
const MARKER_NAME: &str = "arreat_index_explosive_barrel_marker";
const MARKER_ID: i64 = 1_739_039_001;
const PARTICLE_PATH: &str =
    "data/hd/vfx/particles/objects/shrines_other/shrine_baal_magic/vfx_shrine_baal_magic.particles";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticModConfig {
    pub explosive_barrels: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildManifest {
    pub schema_version: u32,
    pub source_build_info_sha256: String,
    pub config: StaticModConfig,
    pub generated_paths: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("此平台暂不支持静态模组运行时；当前仅支持 Linux。")]
    UnsupportedPlatform,
    #[error("D2R 正在运行；请完全退出游戏后重试。")]
    GameRunning,
    #[error("不安全的模组路径：{0}")]
    UnsafePath(String),
    #[error("目标不是 Arreat Index 管理的构建：{0}")]
    UnownedPath(String),
    #[error("本地 D2R 对象格式不兼容：{0}")]
    SourceIncompatible(String),
    #[error("读取本地 D2R 数据失败：{0}")]
    Archive(#[from] arreat_data::Error),
    #[error("文件操作失败（{path}）：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("生成的 JSON 无效（{path}）：{source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("启用新构建失败，且旧构建无法恢复：{0}")]
    RollbackFailed(String),
    #[error("注入的测试故障：{0}")]
    Injected(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Builds and deploys the complete owned static mod below `game_root`.
pub fn apply_local(game_root: &Path, config: StaticModConfig) -> Result<BuildManifest> {
    ensure_supported()?;
    validate_game_root_input(game_root)?;
    refuse_if_running(d2r_is_running()?)?;
    let inputs = with_archive_reader(game_root, |root, build_info, reader| {
        let mut source = None;
        if config.explosive_barrels {
            let mut bytes = Vec::new();
            reader.copy_named(STATIC_MOD_TARGET_PATH, &mut bytes)?;
            source = Some(bytes);
        }
        Ok((root.to_path_buf(), build_info.to_vec(), source))
    })?;
    apply_inputs(
        &inputs.0,
        config,
        &inputs.1,
        inputs.2.as_deref(),
        FailPoint::None,
    )
}

/// Reads and validates the currently applied owned build.
pub fn read_applied(game_root: &Path) -> Result<Option<BuildManifest>> {
    ensure_supported()?;
    let root = canonical_game_root(game_root)?;
    let Some(mod_root) = find_existing_mod_root(&root)? else {
        return Ok(None);
    };
    let active = mod_root.join(ACTIVE_DIRECTORY);
    if !path_entry_exists(&active)? {
        return Ok(None);
    }
    validate_owned_build(&active).map(Some)
}

/// Reports whether a D2R executable is currently present in Linux `/proc`.
pub fn d2r_is_running() -> Result<bool> {
    ensure_supported()?;
    #[cfg(target_os = "linux")]
    {
        for entry in fs::read_dir("/proc").map_err(|source| io_error("/proc", source))? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if !entry
                .file_name()
                .to_string_lossy()
                .bytes()
                .all(|byte| byte.is_ascii_digit())
            {
                continue;
            }
            let name = match fs::read_to_string(entry.path().join("comm")) {
                Ok(name) => name,
                Err(_) => continue,
            };
            if is_d2r_process_name(name.trim()) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[cfg(not(target_os = "linux"))]
    unreachable!()
}

fn ensure_supported() -> Result<()> {
    if cfg!(target_os = "linux") {
        Ok(())
    } else {
        Err(Error::UnsupportedPlatform)
    }
}

fn is_d2r_process_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("d2r.exe") || name.eq_ignore_ascii_case("d2r")
}

fn refuse_if_running(running: bool) -> Result<()> {
    if running {
        Err(Error::GameRunning)
    } else {
        Ok(())
    }
}

fn validate_game_root_input(game_root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(game_root).map_err(|source| io_error(game_root, source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::UnsafePath(game_root.display().to_string()));
    }
    Ok(())
}

fn canonical_game_root(game_root: &Path) -> Result<PathBuf> {
    validate_game_root_input(game_root)?;
    fs::canonicalize(game_root).map_err(|source| io_error(game_root, source))
}

fn apply_inputs(
    root: &Path,
    config: StaticModConfig,
    build_info: &[u8],
    source: Option<&[u8]>,
    failure: FailPoint,
) -> Result<BuildManifest> {
    let target = match (config.explosive_barrels, source) {
        (true, Some(source)) => Some(patch_target(source)?),
        (true, None) => {
            return Err(Error::SourceIncompatible(format!(
                "缺少 {STATIC_MOD_TARGET_PATH}"
            )));
        }
        (false, _) => None,
    };
    let mod_root = prepare_mod_root(root)?;
    let active = mod_root.join(ACTIVE_DIRECTORY);
    let previous = mod_root.join(PREVIOUS_DIRECTORY);
    if path_entry_exists(&active)? {
        validate_owned_build(&active)?;
    }
    if path_entry_exists(&previous)? {
        validate_owned_build(&previous)?;
    }

    failure.hit(FailPoint::CreateStage)?;
    let stage = create_stage(&mod_root)?;
    let mut cleanup = StageCleanup::new(stage.clone());
    let manifest = build_manifest(config, build_info);

    failure.hit(FailPoint::WriteModInfo)?;
    write_json(
        &stage.join(MODINFO_PATH),
        &json!({"name": "Arreat Index", "savepath": "../"}),
    )?;
    if let Some(target) = target {
        failure.hit(FailPoint::WriteTarget)?;
        write_json(&stage.join(STATIC_MOD_TARGET_PATH), &target)?;
    }
    failure.hit(FailPoint::WriteManifest)?;
    write_json(&stage.join(MANIFEST_PATH), &manifest)?;
    failure.hit(FailPoint::Validate)?;
    let validated = validate_owned_build(&stage)?;
    if validated != manifest {
        return Err(Error::UnownedPath(stage.display().to_string()));
    }

    if path_entry_exists(&previous)? {
        failure.hit(FailPoint::RemovePrevious)?;
        fs::remove_dir_all(&previous).map_err(|source| io_error(&previous, source))?;
    }
    let rotated = if path_entry_exists(&active)? {
        failure.hit(FailPoint::Rotate)?;
        fs::rename(&active, &previous).map_err(|source| io_error(&active, source))?;
        true
    } else {
        false
    };
    if let Err(error) = failure
        .hit(FailPoint::Activate)
        .and_then(|()| fs::rename(&stage, &active).map_err(|source| io_error(&stage, source)))
    {
        if rotated {
            fs::rename(&previous, &active)
                .map_err(|source| Error::RollbackFailed(source.to_string()))?;
        }
        return Err(error);
    }
    cleanup.disarm();
    Ok(manifest)
}

fn patch_target(source: &[u8]) -> Result<Value> {
    let mut root: Value = serde_json::from_slice(source).map_err(|source| Error::Json {
        path: STATIC_MOD_TARGET_PATH.to_owned(),
        source,
    })?;
    let object = root
        .as_object_mut()
        .ok_or_else(|| incompatible("根必须是对象"))?;
    if object.get("type").and_then(Value::as_str) != Some("UnitDefinition") {
        return Err(incompatible("根 type 必须是 UnitDefinition"));
    }
    let entities = object
        .get_mut("entities")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| incompatible("entities 必须是数组"))?;
    for entity in entities.iter() {
        if entity.get("name").and_then(Value::as_str) == Some(MARKER_NAME) {
            return Err(incompatible("Arreat 标记已存在"));
        }
        if entity.get("id").and_then(Value::as_i64) == Some(MARKER_ID) {
            return Err(incompatible("Arreat 标记 ID 与本地对象冲突"));
        }
    }
    entities.push(marker_entity());
    Ok(root)
}

fn marker_entity() -> Value {
    json!({
        "type": "Entity",
        "name": MARKER_NAME,
        "id": MARKER_ID,
        "components": [
            {
                "type": "VfxDefinitionComponent",
                "filename": PARTICLE_PATH,
                "hardKillOnDestroy": true
            },
            {
                "type": "TransformDefinitionComponent",
                "position": {"x": 2.0, "y": 1.5, "z": 3.0},
                "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
                "scale": {"x": 1.0, "y": 1.0, "z": 1.0},
                "inheritOnlyPosition": false
            }
        ]
    })
}

fn incompatible(message: &str) -> Error {
    Error::SourceIncompatible(message.to_owned())
}

fn build_manifest(config: StaticModConfig, build_info: &[u8]) -> BuildManifest {
    let mut generated_paths = vec![MANIFEST_PATH.to_owned(), MODINFO_PATH.to_owned()];
    if config.explosive_barrels {
        generated_paths.push(STATIC_MOD_TARGET_PATH.to_owned());
    }
    generated_paths.sort();
    BuildManifest {
        schema_version: 1,
        source_build_info_sha256: format!("{:x}", Sha256::digest(build_info)),
        config,
        generated_paths,
    }
}

fn prepare_mod_root(root: &Path) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for component in ["mods", MOD_DIRECTORY] {
        current.push(component);
        if path_entry_exists(&current)? {
            require_plain_directory(&current)?;
        } else {
            fs::create_dir(&current).map_err(|source| io_error(&current, source))?;
            require_plain_directory(&current)?;
        }
    }
    Ok(current)
}

fn find_existing_mod_root(root: &Path) -> Result<Option<PathBuf>> {
    let mut current = root.to_path_buf();
    for component in ["mods", MOD_DIRECTORY] {
        current.push(component);
        if !path_entry_exists(&current)? {
            return Ok(None);
        }
        require_plain_directory(&current)?;
    }
    Ok(Some(current))
}

fn require_plain_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|source| io_error(path, source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::UnsafePath(path.display().to_string()));
    }
    Ok(())
}

fn path_entry_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(io_error(path, source)),
    }
}

fn create_stage(mod_root: &Path) -> Result<PathBuf> {
    for _ in 0..16 {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random)
            .map_err(|source| Error::SourceIncompatible(format!("无法生成暂存目录名：{source}")))?;
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let stage = mod_root.join(format!(".{ACTIVE_DIRECTORY}.stage-{suffix}"));
        match fs::create_dir(&stage) {
            Ok(()) => return Ok(stage),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error(&stage, source)),
        }
    }
    Err(Error::SourceIncompatible("无法创建唯一暂存目录".to_owned()))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    }
    let bytes = serde_json::to_vec(value).map_err(|source| Error::Json {
        path: relative_display(path),
        source,
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io_error(path, source))?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error(path, source))
}

fn validate_owned_build(directory: &Path) -> Result<BuildManifest> {
    require_plain_directory(directory)
        .map_err(|_| Error::UnownedPath(directory.display().to_string()))?;
    let manifest_path = directory.join(MANIFEST_PATH);
    let metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|_| Error::UnownedPath(directory.display().to_string()))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MANIFEST_BYTES
    {
        return Err(Error::UnownedPath(directory.display().to_string()));
    }
    let bytes = fs::read(&manifest_path).map_err(|source| io_error(&manifest_path, source))?;
    let manifest: BuildManifest = serde_json::from_slice(&bytes)
        .map_err(|_| Error::UnownedPath(directory.display().to_string()))?;
    if manifest.schema_version != 1
        || manifest.source_build_info_sha256.len() != 64
        || !manifest
            .source_build_info_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !strictly_sorted(&manifest.generated_paths)
    {
        return Err(Error::UnownedPath(directory.display().to_string()));
    }
    let actual = collect_files(directory)?;
    let expected = manifest
        .generated_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual != expected || !expected.contains(MANIFEST_PATH) || !expected.contains(MODINFO_PATH) {
        return Err(Error::UnownedPath(directory.display().to_string()));
    }
    let exact_expected = build_manifest(manifest.config, &[])
        .generated_paths
        .into_iter()
        .collect::<BTreeSet<_>>();
    if expected != exact_expected {
        return Err(Error::UnownedPath(directory.display().to_string()));
    }
    for relative in &manifest.generated_paths {
        let path = directory.join(relative);
        let bytes = fs::read(&path).map_err(|source| io_error(&path, source))?;
        serde_json::from_slice::<Value>(&bytes).map_err(|source| Error::Json {
            path: relative.clone(),
            source,
        })?;
    }
    Ok(manifest)
}

fn strictly_sorted(paths: &[String]) -> bool {
    paths.windows(2).all(|pair| pair[0] < pair[1]) && paths.iter().all(|path| safe_relative(path))
}

fn safe_relative(path: &str) -> bool {
    !path.contains('\\')
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn collect_files(root: &Path) -> Result<BTreeSet<String>> {
    fn visit(base: &Path, directory: &Path, output: &mut BTreeSet<String>) -> Result<()> {
        require_plain_directory(directory)?;
        for entry in fs::read_dir(directory).map_err(|source| io_error(directory, source))? {
            let entry = entry.map_err(|source| io_error(directory, source))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|source| io_error(&path, source))?;
            if metadata.file_type().is_symlink() {
                return Err(Error::UnsafePath(path.display().to_string()));
            }
            if metadata.is_dir() {
                visit(base, &path, output)?;
            } else if metadata.is_file() {
                let relative = path.strip_prefix(base).expect("walk stays below base");
                output.insert(relative_display(relative));
            } else {
                return Err(Error::UnsafePath(path.display().to_string()));
            }
        }
        Ok(())
    }
    let mut output = BTreeSet::new();
    visit(root, root, &mut output)?;
    Ok(output)
}

fn relative_display(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> Error {
    Error::Io {
        path: path.into(),
        source,
    }
}

struct StageCleanup {
    path: PathBuf,
    armed: bool,
}

impl StageCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StageCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailPoint {
    None,
    CreateStage,
    WriteModInfo,
    WriteTarget,
    WriteManifest,
    Validate,
    RemovePrevious,
    Rotate,
    Activate,
}

impl FailPoint {
    fn hit(self, current: Self) -> Result<()> {
        if self == current {
            Err(Error::Injected(match current {
                Self::None => "none",
                Self::CreateStage => "create-stage",
                Self::WriteModInfo => "write-modinfo",
                Self::WriteTarget => "write-target",
                Self::WriteManifest => "write-manifest",
                Self::Validate => "validate",
                Self::RemovePrevious => "remove-previous",
                Self::Rotate => "rotate",
                Self::Activate => "activate",
            }))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "arreat-static-mod-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    fn source() -> Vec<u8> {
        br#"{"type":"UnitDefinition","unchanged":{"answer":42},"entities":[{"name":"stock","id":7}]}"#.to_vec()
    }

    fn enabled() -> StaticModConfig {
        StaticModConfig {
            explosive_barrels: true,
        }
    }

    fn apply_test(
        root: &Path,
        config: StaticModConfig,
        failure: FailPoint,
    ) -> Result<BuildManifest> {
        let source = source();
        apply_inputs(
            root,
            config,
            b"build-identity",
            config.explosive_barrels.then_some(source.as_slice()),
            failure,
        )
    }

    fn active(root: &Path) -> PathBuf {
        root.join("mods").join(MOD_DIRECTORY).join(ACTIVE_DIRECTORY)
    }

    fn previous(root: &Path) -> PathBuf {
        root.join("mods")
            .join(MOD_DIRECTORY)
            .join(PREVIOUS_DIRECTORY)
    }

    #[test]
    fn patch_is_minimal_deterministic_and_preserves_unrelated_content() {
        let first = patch_target(&source()).unwrap();
        let second = patch_target(&source()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first["unchanged"], json!({"answer": 42}));
        assert_eq!(first["entities"].as_array().unwrap().len(), 2);
        let marker = first["entities"].as_array().unwrap().last().unwrap();
        assert_eq!(marker["name"], MARKER_NAME);
        assert_eq!(marker["id"], MARKER_ID);
        assert_eq!(marker["components"][0]["filename"], PARTICLE_PATH);
        assert_eq!(marker["components"][0]["hardKillOnDestroy"], true);
        assert_eq!(
            marker["components"][1]["position"],
            json!({"x":2.0,"y":1.5,"z":3.0})
        );
        assert_eq!(marker["components"][1]["inheritOnlyPosition"], false);
        assert!(first.get("dependencies").is_none());
    }

    #[test]
    fn malformed_missing_shapes_duplicate_and_id_collision_are_rejected() {
        for bytes in [
            b"not json".as_slice(),
            br#"[]"#,
            br#"{"type":"Other","entities":[]}"#,
            br#"{"type":"UnitDefinition"}"#,
            br#"{"type":"UnitDefinition","entities":{}}"#,
        ] {
            assert!(patch_target(bytes).is_err());
        }
        let duplicate = format!(
            r#"{{"type":"UnitDefinition","entities":[{{"name":"{MARKER_NAME}","id":2}}]}}"#
        );
        assert!(matches!(
            patch_target(duplicate.as_bytes()),
            Err(Error::SourceIncompatible(_))
        ));
        let collision = format!(
            r#"{{"type":"UnitDefinition","entities":[{{"name":"other","id":{MARKER_ID}}}]}}"#
        );
        assert!(matches!(
            patch_target(collision.as_bytes()),
            Err(Error::SourceIncompatible(_))
        ));
        let root = temp("missing-source");
        assert!(matches!(
            apply_inputs(&root, enabled(), b"build", None, FailPoint::None),
            Err(Error::SourceIncompatible(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enabled_and_disabled_builds_have_exact_deterministic_scope() {
        let root = temp("scope");
        let first = apply_test(&root, enabled(), FailPoint::None).unwrap();
        let modinfo: Value =
            serde_json::from_slice(&fs::read(active(&root).join(MODINFO_PATH)).unwrap()).unwrap();
        assert_eq!(modinfo, json!({"name": "Arreat Index", "savepath": "../"}));
        let first_target = fs::read(active(&root).join(STATIC_MOD_TARGET_PATH)).unwrap();
        let second = apply_test(&root, enabled(), FailPoint::None).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first_target,
            fs::read(active(&root).join(STATIC_MOD_TARGET_PATH)).unwrap()
        );
        assert_eq!(validate_owned_build(&active(&root)).unwrap(), second);
        let off = apply_test(&root, StaticModConfig::default(), FailPoint::None).unwrap();
        assert_eq!(off.generated_paths, [MANIFEST_PATH, MODINFO_PATH]);
        assert!(!active(&root).join(STATIC_MOD_TARGET_PATH).exists());
        assert!(previous(&root).join(STATIC_MOD_TARGET_PATH).exists());
        #[cfg(target_os = "linux")]
        assert_eq!(read_applied(&root).unwrap(), Some(off));
        #[cfg(not(target_os = "linux"))]
        assert!(matches!(
            read_applied(&root),
            Err(Error::UnsupportedPlatform)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unowned_and_symlink_paths_are_never_replaced() {
        let root = temp("unsafe");
        let target = active(&root);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("foreign.txt"), b"owned by user").unwrap();
        assert!(matches!(
            apply_test(&root, enabled(), FailPoint::None),
            Err(Error::UnownedPath(_))
        ));
        assert_eq!(
            fs::read(target.join("foreign.txt")).unwrap(),
            b"owned by user"
        );
        fs::remove_dir_all(&root).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let root = temp("symlink");
            let outside = temp("outside");
            fs::create_dir(root.join("mods")).unwrap();
            symlink(&outside, root.join("mods").join(MOD_DIRECTORY)).unwrap();
            assert!(matches!(
                apply_test(&root, enabled(), FailPoint::None),
                Err(Error::UnsafePath(_))
            ));
            assert!(fs::read_dir(&outside).unwrap().next().is_none());
            fs::remove_dir_all(root).unwrap();
            fs::remove_dir_all(outside).unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn status_refuses_symlinked_mod_ancestors() {
        use std::os::unix::fs::symlink;

        let outside = temp("status-outside");
        apply_test(&outside, enabled(), FailPoint::None).unwrap();

        let mods_link_root = temp("status-mods-link");
        symlink(outside.join("mods"), mods_link_root.join("mods")).unwrap();
        assert!(matches!(
            read_applied(&mods_link_root),
            Err(Error::UnsafePath(_))
        ));

        let mod_link_root = temp("status-mod-link");
        fs::create_dir(mod_link_root.join("mods")).unwrap();
        symlink(
            outside.join("mods").join(MOD_DIRECTORY),
            mod_link_root.join("mods").join(MOD_DIRECTORY),
        )
        .unwrap();
        assert!(matches!(
            read_applied(&mod_link_root),
            Err(Error::UnsafePath(_))
        ));

        fs::remove_dir_all(mods_link_root).unwrap();
        fs::remove_dir_all(mod_link_root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn every_injected_transition_preserves_or_restores_active_and_cleans_stage() {
        let root = temp("failures");
        let original = apply_test(&root, enabled(), FailPoint::None).unwrap();
        apply_test(&root, StaticModConfig::default(), FailPoint::None).unwrap();
        for failure in [
            FailPoint::CreateStage,
            FailPoint::WriteModInfo,
            FailPoint::WriteTarget,
            FailPoint::WriteManifest,
            FailPoint::Validate,
            FailPoint::RemovePrevious,
            FailPoint::Rotate,
            FailPoint::Activate,
        ] {
            let before = validate_owned_build(&active(&root)).unwrap();
            assert!(
                apply_test(&root, enabled(), failure).is_err(),
                "{failure:?}"
            );
            assert_eq!(
                validate_owned_build(&active(&root)).unwrap(),
                before,
                "{failure:?}"
            );
            let names = fs::read_dir(root.join("mods").join(MOD_DIRECTORY))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect::<BTreeSet<_>>();
            assert!(
                names.iter().all(|name| !name.contains("stage")),
                "{failure:?}: {names:?}"
            );
            assert!(names.len() <= 2, "{failure:?}: {names:?}");
            if failure == FailPoint::RemovePrevious {
                assert_eq!(validate_owned_build(&previous(&root)).unwrap(), original);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn process_detection_is_exact_and_never_matches_arguments() {
        assert!(is_d2r_process_name("D2R.exe"));
        assert!(is_d2r_process_name("d2r"));
        assert!(!is_d2r_process_name("launcher.exe"));
        assert!(!is_d2r_process_name("run-D2R.exe-helper"));
        assert!(matches!(refuse_if_running(true), Err(Error::GameRunning)));
        assert!(refuse_if_running(false).is_ok());
    }
}
