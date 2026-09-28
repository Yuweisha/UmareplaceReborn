
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/installer.rc");
    println!("cargo:rerun-if-changed=assets/charreplace.ico");
    println!("cargo:rerun-if-changed=assets/charreplace.manifest");
    println!("cargo:rerun-if-changed=umareplacereborn.dll");

    let version = env!("CARGO_PKG_VERSION").to_string();
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    std::fs::write(
        out_dir.join("version_defs.h"),
        format!(
            "#define VERSION_INFO_STR \"{version}\"\n#define VERSION_INFO_VER {major},{minor},{patch},0\n"
        ),
    )
    .expect("写入 version_defs.h 失败");

    println!("cargo:rustc-env=PACKAGED_VERSION={}", packaged_version());

    #[cfg(windows)]
    embed_resource::compile(
        "assets/installer.rc",
        [out_dir.to_str().expect("OUT_DIR 不是合法路径")],
    );
}

fn packaged_version() -> String {
    use pelite::resources::version_info::{Language, VersionInfo};

    const LANG_NEUTRAL_UNICODE: Language = Language {
        lang_id: 0x0000,
        charset_id: 0x04b0,
    };

    let map = pelite::FileMap::open("umareplacereborn.dll").expect(
        "installer/ 目录下必须有 umareplacereborn.dll（CI 会先从构建产物拷过来）",
    );
    let info: VersionInfo = pelite::PeFile::from_bytes(map.as_ref())
        .expect("读取 umareplacereborn.dll 失败")
        .resources()
        .expect("没有资源节")
        .version_info()
        .expect("没有版本资源 —— 插件构建时是否漏了 build.rs？");

    info.value(LANG_NEUTRAL_UNICODE, "ProductVersion")
        .expect("版本资源里没有 ProductVersion")
        .to_string()
}
