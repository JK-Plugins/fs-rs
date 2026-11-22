use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[rustfmt::skip]
fn main() {
    const EFFECT_VERSION_MAJOR: u32 = 0;
    const EFFECT_VERSION_MINOR: u32 = 0;
    const EFFECT_VERSION_PATCH: u32 = 1;

    // メニューに出る表示名。これは自由に決められる。
    const EFFECT_NAME: &str = "JK Optical Diffusion F's";

    // AE がエフェクトを同定するキー。元の F's OpticalDiffusion と同じ値にしてある
    // ので、元を使っている既存の .aep はこのプラグインを読み込み、保存済みの
    // パラメータ値がそのまま入る（AE は値をスロット順で復元する。26スロットの
    // 順序・型・範囲が元と一致していることを確認済み）。
    //
    // この値を変えると既存プロジェクトとの互換が切れる。また、元の .aex と
    // 同じ matchName なので両方を MediaCore に入れると AE が重複を警告し
    // どちらが読まれるか不定になる。これは置き換えであって共存はしない。
    // 元と出力を比較したいときは、元だけを別環境に入れて比べること。
    const EFFECT_MATCH_NAME: &str = "F's OpticalDiffusion";

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
        Property::AE_Effect_Support_URL("https://github.com/JK-Plugins"),
    ]);
}
