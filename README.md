# hachimi-charreplace-plugin

把游戏里的角色全局替换成别的角色 —— 从 [Trainers' Legend G](https://github.com/MinamiChiwa/Trainers-Legend-G) 的 `replaceGlobalChar` 移植，做成 [Hachimi](https://github.com/kairusds/Hachimi-Edge) 的**外置插件**。

好处是功能独立于 Hachimi 本体：Hachimi 更新时只要插件接口版本不变，这个 dll 不用重新编译。

## 功能

- 把指定角色在游戏里替换成另一个角色（含服装、头部模型）
- 可选是否同时替换服装、是否作用于迷你角色
- 比赛服装（`GetRaceDressId`）也跟着替换
- 配置格式与 TLG 的 `replaceGlobalChar` 完全兼容，可以直接把 TLG 的配置段粘过来
- 游戏内菜单：配置编辑器里多出一页「角色替换」，角色和服装都是从 `master.mdb` 读出来的**带搜索的下拉**，**可以直接搜角色名或 ID**（例如搜 `1114` 就是迷人景致）
- 数据库直接用 rusqlite 读文件，不走游戏内部接口

## 安装

1. 把 `charreplace.dll` 放到游戏根目录（和 `UmamusumePrettyDerby_Jpn.exe` 同一层）。
2. 编辑 `<游戏目录>/hachimi/config.json`，在 `windows.load_libraries` 里加上文件名：

```json
{
    "windows": {
        "load_libraries": [
            "charreplace.dll"
        ]
    }
}
```

3. 启动游戏，打开菜单（默认右方向键）→ 配置编辑器 → **角色替换**。

> **注意**：如果 Hachimi 本体里已经带了同样的角色替换功能（自编译版本），请换回官方 hachimi.dll 再用本插件，否则两套替换逻辑会互相干扰。

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

生效的场景与 TLG 一致：除了默认、家中对话/走动、以及迷你场景以外的控制器都会被替换。

日志写在 `hachimi.log` 里，搜 `charreplace` 就能看到插件加载、hook、数据库加载的情况。

## 从源码构建

```bash
cargo build --release
# 产物：target/release/charreplace.dll
```

需要 Rust（MSVC 工具链）。依赖只有 serde、serde_json、rusqlite（bundled sqlite）、once_cell。

## 实现说明

- 通过 Hachimi 插件接口 v3（`hachimi_init_v3`）接入，使用的 API：`interceptor_hook` 挂钩、`il2cpp_*` 取类/字段、`gui_*` 画界面、`hachimi_get_data_path` 定位 `master.mdb`。
- 挂钩 `Gallop.CharacterBuildInfo::Rebuild`，在模型构建前改写 `_charaId` / `_dressId` / `_headModelSubId` / `_motionDressId`，并把 `_cardId` 置 -1；另外挂钩 `Gallop.WorkSingleModeCharaData::GetRaceDressId` 让比赛服装也走替换表。
- 服装的头部模型与迷你模型信息来自 `master.mdb` 的 `dress_data`（`head_sub_id`、`have_mini`），启动时一次性缓存。

## 许可

GPL-3.0-or-later（与 Hachimi 及 Trainers' Legend G 保持一致）。
