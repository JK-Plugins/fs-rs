use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

/// Cargo.toml の version を AE のエフェクトバージョンに変換する。
/// AE_Effect_Version はビット幅が小さい（major 0-127 / minor 0-15 / patch 0-15）ので、
/// 範囲外ならビルドを止める。
fn effect_version() -> (u32, u32, u32) {
    let part = |name: &str, max: u32| -> u32 {
        let value: u32 = std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} is not set"))
            .parse()
            .unwrap_or_else(|_| panic!("{name} is not a number"));
        assert!(
            value <= max,
            "{name}={value} exceeds the After Effects limit ({max}). Adjust version in Cargo.toml."
        );
        value
    };
    (
        part("CARGO_PKG_VERSION_MAJOR", 127),
        part("CARGO_PKG_VERSION_MINOR", 15),
        part("CARGO_PKG_VERSION_PATCH", 15),
    )
}

#[rustfmt::skip]
fn main() {
    // バージョンは Cargo.toml の version だけで管理する
    let (effect_version_major, effect_version_minor, effect_version_patch) = effect_version();

    // メニューに出る表示名。
    const EFFECT_NAME: &str = "JK Color Change Simple F's";

    // AE がエフェクトを同定するキー。元の F's プラグインと同じ値にしてあるので、
    // 元を使っている既存の .aep はこのプラグインを読み込み、保存済みの値がそのまま
    // 入る (AE は値をスロット順で復元する。構成が元と一致していることを確認済み)。
    // 元と同じ matchName なので両方を MediaCore に入れると AE が重複を警告し、
    // どちらが読まれるか不定になる。これは置き換えであって共存はしない。
    const EFFECT_MATCH_NAME: &str = "F's ColorChangeSimple";

    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name(EFFECT_NAME),
        Property::Category("JK Plugins F's"),

        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),

        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: PF_PLUG_IN_VERSION, minor: PF_PLUG_IN_SUBVERS },
        Property::AE_Effect_Version {
            version: effect_version_major,
            subversion: effect_version_minor,
            bugversion: effect_version_patch,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::PixIndependent |
            OutFlags::NonParamVary |
            OutFlags::DeepColorAware
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::FloatColorAware |
            OutFlags2::SupportsSmartRender |
            OutFlags2::SupportsThreadedRendering |
            OutFlags2::SupportsGetFlattenedSequenceData
        ),
        Property::AE_Effect_Match_Name(EFFECT_MATCH_NAME),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL("https://www.adobe.com"),
    ]);
}
