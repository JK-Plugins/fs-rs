use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[rustfmt::skip]
fn main() {
    const EFFECT_VERSION_MAJOR: u32 = 0;
    const EFFECT_VERSION_MINOR: u32 = 0;
    const EFFECT_VERSION_PATCH: u32 = 1;

    // メニューに出る表示名。
    const EFFECT_NAME: &str = "JK Alpha Hyperbolic F's";

    // matchName はまだ元の "F's AlphaHyperbolic" に揃えていない。
    //
    // 揃えると AE は元で保存した .aep をこのプラグインで開くが、唯一の
    // パラメータ hyperbolic の値が復元されず既定値 5 に戻ってしまう
    // (AE 2023 実機で確認。元のプラグインで 3 を入れて保存し、元だけを入れた
    // 状態では 3 が読めるのに、こちらだけを入れると 5 になる)。
    //
    // パラメータ構成は元と完全一致していて (float / -10..50 / 既定 5)、
    // 原因は切り分け済みのものを除いて未特定:
    //   - Parameters::add が enum 変体名のハッシュから作る id ... 変体名を変えても再現
    //   - スライダー範囲が入力範囲より狭いこと ... 一致させても再現
    // 残る容疑は precision / curve tolerance / 各種フラグあたり。
    //
    // 値が黙って既定値に戻るのは、互換が無いより悪い (ユーザーのプロジェクトの
    // 見た目が勝手に変わる)。原因が分かるまで独立した matchName のままにする。
    const EFFECT_MATCH_NAME: &str = "JK Alpha Hyperbolic F's";

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
            version: EFFECT_VERSION_MAJOR,
            subversion: EFFECT_VERSION_MINOR,
            bugversion: EFFECT_VERSION_PATCH,
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
