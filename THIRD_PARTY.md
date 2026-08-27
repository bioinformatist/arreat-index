# Third-party code and data

Repository-authored code is licensed under the MIT License. That license does
not grant rights to Blizzard game data, localized strings, binary assets, or
third-party catalogs and translations.

The Linux exporter links CascLib 3.0, copyright Ladislav Zezula and CascLib
contributors, under its MIT License. Nix pins upstream tag `3.0` at commit
`4971d363e665551ac4142f541e5f2d71f1cda653`.

The experimental client plugin builds against the D2RLoader Plugin SDK under
its MIT License. CMake fetches the SDK only at build time and pins commit
`4933e2c42cb2592958cd0df3b6dc5003102252d1`; headers are not vendored. The exact
license text is retained at
`plugins/d2rloader/LICENSE.D2RLoader-PluginSDK.txt` and staged beside the DLL.
The D2RLoader binary is not fetched or distributed. Project evidence contains
no verified redistributable license for that binary, so users must obtain it
independently under the terms that apply to them.

No full game table, normalized full snapshot, marketplace catalog, or community
database is distributed here. Small fixtures under `tests/fixtures` were
written specifically to test this repository. Contributors must supply their
own local game input, review the terms that apply to them, and must not commit
or upload extracted source data or derived full snapshots.
