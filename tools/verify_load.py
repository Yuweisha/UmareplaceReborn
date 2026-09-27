"""无需游戏的加载自检：用 ctypes 模拟 Hachimi 的插件加载流程。

验证内容：导出符号 hachimi_init_v3 可调用、插件请求的 24 个 API 名字全部存在、
配置可从 hachimi/config.json 的 replaceGlobalChar 迁移、真实读取 master.mdb
（角色/服装/迷你标记）、两个 hook 的安装调用、以及菜单界面的绘制调用与下拉内容。

用法：cargo build --release 后运行 python tools/verify_load.py
需要本机装有游戏（会以只读方式打开 master.mdb）。
"""

"""完整模拟 Hachimi 加载 charreplace.dll：提供全部 24 个 API stub，验证
初始化、配置迁移、master.mdb 读取、hook 安装路径和菜单界面的绘制调用。"""

import ctypes
import json
import os
import shutil
import time

DLL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "release", "charreplace.dll")
GAME_DATA = r"C:\Users\Schwarz\AppData\LocalLow\Cygames\UmamusumePrettyDerby_Jpn"
TESTENV = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_testenv")
HACHIMI_DIR = os.path.join(TESTENV, "hachimi")

if os.path.isdir(TESTENV):
    shutil.rmtree(TESTENV, ignore_errors=True)
os.makedirs(HACHIMI_DIR, exist_ok=True)
with open(os.path.join(HACHIMI_DIR, "config.json"), "w", encoding="utf-8") as f:
    json.dump({
        "replaceGlobalChar": {
            "enable": True,
            "replaceUniversal": False,
            "data": [{"origCharId": 1046, "newChrId": 1030, "newClothId": 103001, "replaceMini": False}],
        }
    }, f)
os.makedirs(os.path.join(HACHIMI_DIR, "localized_data"), exist_ok=True)
with open(os.path.join(HACHIMI_DIR, "localized_data", "text_data_dict.json"), "w", encoding="utf-8") as f:
    json.dump({"170": {"1114": "迷人景致", "1030": "米浴", "1046": "待兼诗歌剧"},
               "5": {"103001": "[ローゼスドリーム]"}}, f, ensure_ascii=False)

lib = ctypes.WinDLL(DLL)
keep = []
logs = []
looked = []
calls = []

def cb_of(fn_type, impl):
    c = fn_type(impl)
    keep.append(c)
    return ctypes.cast(c, ctypes.c_void_p).value

Void = ctypes.c_void_p
LogFn = ctypes.CFUNCTYPE(None, ctypes.c_int, ctypes.c_char_p, ctypes.c_char_p)
GetApi = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_char_p)
PathFn = ctypes.CFUNCTYPE(ctypes.c_void_p)
InstFn = ctypes.CFUNCTYPE(ctypes.c_void_p)
InterceptFn = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p)
GameInitCb = ctypes.CFUNCTYPE(None, ctypes.c_void_p)
RegGameInit = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p)
RegSection = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_char_p, ctypes.c_char_p,
                              ctypes.POINTER(ctypes.c_ubyte), ctypes.c_size_t,
                              ctypes.c_void_p, ctypes.c_void_p)
AssemblyFn = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_char_p)
ClassFn = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p)
MethodFn = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int)
FieldFn = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_char_p)
VoidText = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_char_p)
BoolText = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_char_p)
VoidUi = ctypes.CFUNCTYPE(None, ctypes.c_void_p)
CheckFn = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_bool))
HorizFn = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p)
ComboFn = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_char_p,
                           ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_char_p),
                           ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t)
ColoredFn = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_ubyte, ctypes.c_ubyte,
                             ctypes.c_ubyte, ctypes.c_ubyte, ctypes.c_char_p)

BASE = ctypes.create_string_buffer(HACHIMI_DIR.encode())
DATA = ctypes.create_string_buffer(GAME_DATA.encode())
state = {"game_init": None, "section": None, "icon": None, "icon_magic": None, "title": None}
FAKE_CLASS = 0x3000
FAKE_ADDR = 0x4000
FAKE_TRAMPOLINE = 0x5000

def f_log(level, target, message):
    entry = (level, message.decode() if message else "")
    logs.append(entry)

def f_instance():
    return 0x1000

def f_interceptor(this):
    return 0x2000

def f_intercept_hook(this, orig, hook):
    calls.append(f"interceptor_hook(orig=0x{orig:x}, hook=0x{hook:x})")
    return FAKE_TRAMPOLINE

def f_base():
    return ctypes.addressof(BASE)

def f_data():
    return ctypes.addressof(DATA)

def f_reg_game_init(cb, ud):
    state["game_init"] = ctypes.cast(cb, GameInitCb)
    calls.append("register_on_game_initialized")

def f_reg_section(title, uri, icon_ptr, icon_len, cb, ud):
    state["title"] = title.decode()
    state["section"] = ctypes.cast(cb, ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p))
    state["icon"] = icon_len
    state["icon_magic"] = bytes(icon_ptr[:8])
    calls.append(f"register_menu_section(title={title.decode()!r}, icon={icon_len}B, uri={uri.decode() if uri else None})")
    return True

def f_assembly(name):
    return 0x9000  # 假装 assembly 已加载

def f_class(image, ns, name):
    calls.append(f"get_class({ns.decode()}.{name.decode()})")
    return FAKE_CLASS

def f_method(cls, name, argc):
    calls.append(f"get_method_addr({name.decode()}, argc={argc})")
    return FAKE_ADDR

def f_field(cls, name):
    return 0x6000

def f_void_ui(ui):
    return None

def f_void_text(ui, text):
    calls.append(f"  ui_text({text.decode()!r})")

def f_bool_text(ui, text):
    calls.append(f"  ui_button({text.decode()!r})")
    return False

def f_checkbox(ui, text, value):
    calls.append(f"  ui_checkbox({text.decode()!r}, 当前={bool(value[0])})")
    return False

def f_horizontal(ui, cb, ud):
    fn = ctypes.cast(cb, ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p))
    fn(ui, ud)
    return True

def f_combo(ui, id_, sel_ptr, items, count, search, slen):
    idx = sel_ptr[0]
    arr = ctypes.cast(items, ctypes.POINTER(ctypes.c_char_p * count))[0]
    labels = [arr[i].decode() for i in range(count)]
    hits = [l for l in labels if "1114" in l]
    calls.append(f"  combo({id_.decode()}): {count} 项, 选中索引 {idx}, "
                 f"搜索 1114 → {hits}")
    if count > 3:
        calls.append(f"    前 3 项: {labels[:3]}")
    return False

def f_colored(ui, r, g, b, a, text):
    return None

API = {
    "log": cb_of(LogFn, f_log),
    "hachimi_instance": cb_of(InstFn, f_instance),
    "hachimi_get_interceptor": cb_of(ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p), f_interceptor),
    "interceptor_hook": cb_of(InterceptFn, f_intercept_hook),
    "hachimi_get_base_dir": cb_of(PathFn, f_base),
    "hachimi_get_data_path": cb_of(PathFn, f_data),
    "hachimi_register_on_game_initialized": cb_of(RegGameInit, f_reg_game_init),
    "gui_register_menu_section_with_icon": cb_of(RegSection, f_reg_section),
    "il2cpp_get_assembly_image": cb_of(AssemblyFn, f_assembly),
    "il2cpp_get_class": cb_of(ClassFn, f_class),
    "il2cpp_get_method_addr": cb_of(MethodFn, f_method),
    "il2cpp_get_field_from_name": cb_of(FieldFn, f_field),
    "il2cpp_get_field_value": cb_of(ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p), lambda o, f, out: None),
    "il2cpp_set_field_value": cb_of(ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p), lambda o, f, v: None),
    "gui_ui_heading": cb_of(VoidText, f_void_text),
    "gui_ui_label": cb_of(VoidText, f_void_text),
    "gui_ui_small": cb_of(VoidText, f_void_text),
    "gui_ui_separator": cb_of(VoidUi, f_void_ui),
    "gui_ui_button": cb_of(BoolText, f_bool_text),
    "gui_ui_checkbox": cb_of(CheckFn, f_checkbox),
    "gui_ui_horizontal": cb_of(HorizFn, f_horizontal),
    "gui_ui_colored_label": cb_of(ColoredFn, f_colored),
    "gui_ui_combo_menu": cb_of(ComboFn, f_combo),
    "gui_show_notification": cb_of(VoidText, f_void_text),
}
looked_names = []

@GetApi
def get_api(name):
    key = name.decode()
    looked_names.append(key)
    return API.get(key, 0)

print("=== 1) hachimi_init_v3 ===")
init = lib.hachimi_init_v3
init.restype = ctypes.c_int
init.argtypes = [ctypes.c_void_p, ctypes.c_int]
res = init(ctypes.cast(get_api, ctypes.c_void_p).value, 3)
print(f"  返回 {res} (1=Ok)；查找了 {len(looked_names)} 个 API")
unknown = [n for n in looked_names if n not in API]
print(f"  未提供 stub 的名字: {unknown or '无'}")
print(f"  菜单节: {calls[0] if calls else '无'}")
print(f"  图标头部字节: {state['icon_magic']!r} (PNG 应为 b'\\x89PNG\\r\\n\\x1a\\n')")

print("\n=== 2) 触发 game_initialized（真实读取 master.mdb）===")
t0 = time.time()
state["game_init"](None)
time.sleep(4)
print(f"  耗时 {time.time() - t0:.1f}s")
for level, message in logs:
    print(f"  [log{level}] {message}")

print("\n=== 3) master.mdb 读取结果（通过 hook 安装日志确认）===")
print("  配置文件:", os.path.join(HACHIMI_DIR, "charreplace.json"))
if os.path.exists(os.path.join(HACHIMI_DIR, "charreplace.json")):
    with open(os.path.join(HACHIMI_DIR, "charreplace.json"), encoding="utf-8") as f:
        print(f.read())

print("=== 4) hook / 菜单注册调用记录 ===")
for c in calls:
    print("  " + c)

print("\n=== 5) 渲染菜单节（模拟 Hachimi 调用 section 回调）===")
calls.clear()
state["section"](0x7000, None)
for c in calls:
    print("  " + c)
