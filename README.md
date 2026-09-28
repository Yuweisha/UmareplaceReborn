# UMA角色替换Reborn

鸣谢、代码参考、灵感来源:Trainer-Legend-G
把游戏里的角色全局替换成别的角色，作为Hachimi-Edge下挂载的独立插件。
- 优点:原生、易于安装、不随本体更新而需要重新patch
- 缺点:只能替换角色，无法替换音轨，其他的暂时还没测出来

## 功能

- 把指定角色在游戏里替换成另一个角色指定的服装
- 可选是否同时替换服装、是否作用于迷你角色
- 比赛服装（`GetRaceDressId`）也跟着替换
- 配置格式与 TLG 的 `replaceGlobalChar` 完全兼容，可以直接把 TLG 的配置段粘过来
- 游戏内菜单：配置编辑器下新增选项「角色替换」，角色和服装从 `master.mdb` 直接读取**带搜索的下拉**，**可以直接搜角色名或 ID**（例如搜 `1114` 就是迷人景致）
- 数据库直接用 rusqlite 读文件，不走游戏内部接口

## 安装

### Windows

1. 把 `charreplace.dll` 放进 `<游戏根目录>` 文件夹。(例如UmamusumePrettyDerby_Jpn)
2. 在 `<游戏目录>/hachimi/config.json` 的 `load_libraries` 里加上路径（顶层键，Hachimi 自己也是写在这里）：

```json
    "load_libraries": ["charreplace.dll"]
```

3. 重启游戏，打开菜单（默认右方向键）→ 配置编辑器 → **角色替换**。

### Android

安卓上插件通过 **UmaPatcher Edge** 添加，不需要手动往游戏目录塞文件：

1. 准备 `libhachimi_charreplace.so`（arm64 设备）。
2. 打开 UmaPatcher Edge → 向下滚动到「插件」部分 → 「添加插件」→ 选择这个 .so 文件。
3. 按平常的流程补丁游戏，然后启动。

插件会保留在 UmaPatcher Edge 里，游戏更新后重新补丁即可，不用重新添加。

## 配置

首次运行时，插件会尝试从 `<游戏目录>/hachimi/config.json` 里的 `replaceGlobalChar` 段导入现有配置，然后写到自己的文件：

```
<游戏目录>/hachimi/charreplace.json
```

格式（字段名与 TLG 一致，也接受 `replace_global_char` 之类的下划线写法）：

```json
{
    "enable": true,
    "replace_universal": true,
    "replace_voice": true,
    "data": [
        {
            "origCharId": 1046,
            "newChrId": 1030,
            "newClothId": 103001,
            "replaceMini": false
        }
    ]
}
```

| 字段 | 说明 |
| --- | --- |
| `enable` | 总开关，对应菜单里的「启用角色替换」 |
| `replaceUniversal` | 是否连服装一起换。关闭时，原服装 ID 小于 100000 的只换角色、保留原服装 |
| `origCharId` | 要被替换掉的角色 ID |
| `newChrId` | 替换成哪个角色 |
| `newClothId` | 用哪个服装（服装 ID 一般是 `<角色 ID> * 100 + 序号`） |
| `replaceMini` | 这条规则是否也作用于迷你角色 |
| `replace_voice` | 是否同时替换语音（默认开）。只影响 cue 名里带角色 ID 的语音 |
| `log_audio_cues` | 诊断开关：把游戏播放的每个 cue 记进日志，用来定位还不支持的音轨（例如 live 歌曲） |

生效的场景与 TLG 一致：除了默认、家中对话/走动、以及迷你场景以外的控制器都会被替换。

日志写在 `hachimi.log` 里，搜 `charreplace` 就能看到插件加载、hook、数据库加载的情况。

## 从源码构建

Windows：

```bash
cargo build --release
# 产物：target/release/charreplace.dll
```

Android（需要 Android NDK，用 [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) 自动配置工具链）：

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
cargo ndk -t arm64-v8a build --release
# 产物：target/aarch64-linux-android/release/libcharreplace.so
```

代码本身不含平台相关分支，两个平台共用同一份源码。依赖只有 serde、serde_json、rusqlite（bundled sqlite）、once_cell。

推 tag（`v*`）时 GitHub Actions 会自动构建两个平台并发布 Release；仓库里的 `tools/verify_load.py` 可以在不启动游戏的情况下验证 Windows dll 的加载流程。

## 实现说明

- 通过 Hachimi 插件接口 v3（`hachimi_init_v3`）接入，使用的 API：`interceptor_hook` 挂钩、`il2cpp_*` 取类/字段、`gui_*` 画界面、`hachimi_get_data_path` 定位 `master.mdb`。
- 挂钩 `Gallop.CharacterBuildInfo::Rebuild`，在模型构建前改写 `_charaId` / `_dressId` / `_headModelSubId` / `_motionDressId`，并把 `_cardId` 置 -1；另外挂钩 `Gallop.WorkSingleModeCharaData::GetRaceDressId` 让比赛服装也走替换表。
- 服装的头部模型与迷你模型信息来自 `master.mdb` 的 `dress_data`（`head_sub_id`、`have_mini`），启动时一次性缓存。
