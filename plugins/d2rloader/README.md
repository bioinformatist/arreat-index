# D2RLoader tooltip identity proof

This directory contains an experimental, client-only D2RLoader plugin. It
observes the final item-tooltip event and appends copied identity facts for a
supported rune, Unique item, or Set item. It does not resolve canonical names,
query DD373, calculate prices, or provide the planned production price card.

The build requires Windows x64, CMake 3.28 or newer, and MSVC or clang-cl. From
the repository root, configure, build, test, and stage the package with:

```powershell
cmake -S plugins/d2rloader -B build/d2rloader -A x64 -DD2RLPLUGIN_BUILD_EXAMPLES=OFF -DD2RLPLUGIN_BUILD_TESTS=OFF
cmake --build build/d2rloader --config Release --parallel
ctest --test-dir build/d2rloader -C Release --output-on-failure
cmake --install build/d2rloader --config Release --prefix stage/d2rloader-proof
```

CMake fetches the MIT-licensed D2RLoader Plugin SDK at the exact commit
`4933e2c42cb2592958cd0df3b6dc5003102252d1`; its examples and tests stay
disabled. The staged tree contains only:

```text
d2rloader/plugins/d2rl-arreat-index.dll
licenses/LICENSE.D2RLoader-PluginSDK.txt
```

D2RLoader itself must be downloaded independently. This repository neither
fetches nor distributes the loader binary. Installation, loader configuration,
and CN Proton runtime testing are intentionally outside this build procedure.
A successful build proves only ABI compilation. Physical acceptance must still
observe a loose rune, a stacked-stash rune entry, one fixed Unique item, and one
fixed Set item with the exact reviewed DLL.
