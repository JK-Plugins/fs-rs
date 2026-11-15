// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/FsLibrary/FsUtils.h

/*

    AfterEffectsSDKの型一覧です。

    typedef int32_t			A_long;
    typedef uint32_t		A_u_long;
    typedef char			A_char;
    typedef double			A_FpLong;
    typedef float			A_FpShort;
    typedef A_long			A_Err;
    typedef void *			A_Handle;
    typedef A_long			A_Fixed;
    typedef A_u_long		A_UFixed;

    #if defined( __MWERKS__) || defined (__GNUC__)  // metrowerks codewarrior and XCode/GCC
        typedef int16_t			A_short;
        typedef uint16_t		A_u_short;
        typedef uint8_t			A_u_char;
        typedef uint8_t			A_Boolean;
        typedef intptr_t		A_intptr_t;
    #else // windows
        typedef short			A_short;
        typedef unsigned short	A_u_short;
        typedef unsigned char	A_u_char;
        typedef unsigned char	A_Boolean;
        #ifdef  _WIN64
            typedef __int64     A_intptr_t;
        #else
            typedef  int32_t       A_intptr_t;
        #endif
    #endif
*/

use after_effects::{
    sys::{PF_Pixel, PF_Pixel16, PF_PixelFloat},
    HALF_CHANNEL16, HALF_CHANNEL8, MAX_CHANNEL16, MAX_CHANNEL8,
};

pub fn abs<T>(x: T) -> T
where
    T: PartialOrd + std::ops::Neg<Output = T> + Copy + Default,
{
    if x >= T::default() {
        x
    } else {
        -x
    }
}

pub fn comp_pix8_lv(s: &PF_Pixel, d: &PF_Pixel, lv: u8) -> bool {
    (s.blue as i32 - d.blue as i32).abs() <= lv as i32
        && (s.green as i32 - d.green as i32).abs() <= lv as i32
        && (s.red as i32 - d.red as i32).abs() <= lv as i32
}

pub fn round_byte_long(x: i32) -> u8 {
    let mut temp = x;
    if temp < 0 {
        temp = 0;
    }
    if temp > MAX_CHANNEL8 as i32 {
        temp = MAX_CHANNEL8 as i32;
    }
    temp as u8
}

/// 元の `RoundByteFpLong`。名前に反して**丸めない**。クランプしてから
/// `(A_u_char)temp` でキャストするだけなので切り捨てになる。
///
/// ```c
/// inline A_u_char RoundByteFpLong(PF_FpLong x) {
///     PF_FpLong temp=x;
///     if (temp<0)   temp=0;
///     if (temp>PF_MAX_CHAN8) temp=PF_MAX_CHAN8;
///     return (A_u_char)temp;
/// }
/// ```
///
/// ここで四捨五入していたせいで、元と最大 1 ずれる画素が出ていた
/// (RGBAControl で 31049 px、AEXCompat の比較で判明)。
pub fn round_byte_fp_long(x: f32) -> u8 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > MAX_CHANNEL8 as f32 {
        temp = MAX_CHANNEL8 as f32;
    }
    temp as u8
}

/// 元の `RoundShortFpLong`。こちらも丸めずに切り捨てる。
pub fn round_short_fp_long(x: f32) -> u16 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > MAX_CHANNEL16 as f32 {
        temp = MAX_CHANNEL16 as f32;
    }
    temp as u16
}

/// 元の RoundByteFpLong は PF_FpLong (double) を取る。f32 版に値を渡すと
/// 丸め境界で 1 ずれることがあるので、元が double のまま計算している
/// ところではこちらを使う。
pub fn round_byte_fp_double(x: f64) -> u8 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > MAX_CHANNEL8 as f64 {
        temp = MAX_CHANNEL8 as f64;
    }
    temp as u8
}

/// 元の RoundShortFpLong の double 版。
pub fn round_short_fp_double(x: f64) -> u16 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > MAX_CHANNEL16 as f64 {
        temp = MAX_CHANNEL16 as f64;
    }
    temp as u16
}

pub fn round_short(x: i32) -> u16 {
    let mut temp = x;
    if temp < 0 {
        temp = 0;
    }
    if temp > MAX_CHANNEL16 as i32 {
        temp = MAX_CHANNEL16 as i32;
    }
    temp as u16
}

pub fn round_fp_short(x: f32) -> f32 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > 32.0 {
        temp = 32.0;
    }
    temp
}

pub fn conv_16_to_8(p: &PF_Pixel16) -> PF_Pixel {
    //#define FS_CONVERT16TO8(A)		( (((A_long)(A) * PF_MAX_CHAN8) + PF_HALF_CHAN16) / PF_MAX_CHAN16)
    PF_Pixel {
        alpha: round_byte_fp_long(
            (((p.alpha as f32) * (MAX_CHANNEL8 as f32)) + HALF_CHANNEL16 as f32)
                / MAX_CHANNEL16 as f32,
        ),
        red: round_byte_fp_long(
            (((p.red as f32) * (MAX_CHANNEL8 as f32)) + HALF_CHANNEL16 as f32)
                / MAX_CHANNEL16 as f32,
        ),
        green: round_byte_fp_long(
            (((p.green as f32) * (MAX_CHANNEL8 as f32)) + HALF_CHANNEL16 as f32)
                / MAX_CHANNEL16 as f32,
        ),
        blue: round_byte_fp_long(
            (((p.blue as f32) * (MAX_CHANNEL8 as f32)) + HALF_CHANNEL16 as f32)
                / MAX_CHANNEL16 as f32,
        ),
    }
}

pub fn conv_32_to_8(p: &PF_PixelFloat) -> PF_Pixel {
    PF_Pixel {
        alpha: round_byte_fp_long(p.alpha * (MAX_CHANNEL8 as f32) + 0.5),
        red: round_byte_fp_long(p.red * (MAX_CHANNEL8 as f32) + 0.5),
        green: round_byte_fp_long(p.green * (MAX_CHANNEL8 as f32) + 0.5),
        blue: round_byte_fp_long(p.blue * (MAX_CHANNEL8 as f32) + 0.5),
    }
}

pub fn conv_8_to_16(p: &PF_Pixel) -> PF_Pixel16 {
    PF_Pixel16 {
        //#define FS_CONVERT8TO16(A)		( (((A_long)(A) * PF_MAX_CHAN16) + PF_HALF_CHAN8) / PF_MAX_CHAN8 )
        alpha: round_short_fp_long(
            (((p.alpha as f32) * (MAX_CHANNEL16 as f32)) + HALF_CHANNEL8 as f32)
                / MAX_CHANNEL8 as f32,
        ),
        red: round_short_fp_long(
            (((p.red as f32) * (MAX_CHANNEL16 as f32)) + HALF_CHANNEL8 as f32)
                / MAX_CHANNEL8 as f32,
        ),
        green: round_short_fp_long(
            (((p.green as f32) * (MAX_CHANNEL16 as f32)) + HALF_CHANNEL8 as f32)
                / MAX_CHANNEL8 as f32,
        ),
        blue: round_short_fp_long(
            (((p.blue as f32) * (MAX_CHANNEL16 as f32)) + HALF_CHANNEL8 as f32)
                / MAX_CHANNEL8 as f32,
        ),
    }
}

/// 元の FS_CONVERT8TO32。10000倍して四捨五入し、整数に落としてから
/// 10000 で割る: つまり小数4桁に量子化される。`(long)` のキャストを
/// 落とすと量子化されず、Lab 変換のように敏感な処理で 8bit 出力が 1
/// ずれる。
pub fn conv_8_to_32_channel(c: u8) -> f32 {
    ((c as f64 * 10000.0 / MAX_CHANNEL8 as f64 + 0.5) as i64 as f64 / 10000.0) as f32
}

/// 元の FS_CONVERT16TO32。8bit 版と同じく小数4桁に量子化する。
pub fn conv_16_to_32_channel(c: u16) -> f32 {
    ((c as f64 * 10000.0 / MAX_CHANNEL16 as f64 + 0.5) as i64 as f64 / 10000.0) as f32
}

pub fn conv_8_to_32(p: &PF_Pixel) -> PF_PixelFloat {
    PF_PixelFloat {
        alpha: conv_8_to_32_channel(p.alpha),
        red: conv_8_to_32_channel(p.red),
        green: conv_8_to_32_channel(p.green),
        blue: conv_8_to_32_channel(p.blue),
    }
}

pub fn conv_16_to_32(p: &PF_Pixel16) -> PF_PixelFloat {
    PF_PixelFloat {
        alpha: conv_16_to_32_channel(p.alpha),
        red: conv_16_to_32_channel(p.red),
        green: conv_16_to_32_channel(p.green),
        blue: conv_16_to_32_channel(p.blue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 元の FS_CONVERT8TO32 は小数4桁に量子化する。
    #[test]
    fn conv_8_to_32_quantises_to_four_decimals() {
        // 1/255 = 0.00392156.. -> 0.0039
        assert_eq!(conv_8_to_32_channel(1), 0.0039);
        // 128/255 = 0.50196.. -> 0.502
        assert_eq!(conv_8_to_32_channel(128), 0.502);
        assert_eq!(conv_8_to_32_channel(0), 0.0);
        assert_eq!(conv_8_to_32_channel(255), 1.0);
    }

    #[test]
    fn conv_16_to_32_quantises_to_four_decimals() {
        assert_eq!(conv_16_to_32_channel(0), 0.0);
        // AE の 16bit は 0..32768 (PF_MAX_CHAN16)。
        assert_eq!(conv_16_to_32_channel(MAX_CHANNEL16 as u16), 1.0);
        assert_eq!(conv_16_to_32_channel(16384), 0.5);
    }

    /// 元の Round* 系は名前に反して丸めず、クランプしてキャストするだけ。
    /// つまり切り捨てになる。四捨五入にすると元と最大 1 ずれる。
    #[test]
    fn round_helpers_truncate_rather_than_round() {
        assert_eq!(round_byte_fp_long(1.9), 1);
        assert_eq!(round_byte_fp_long(0.999), 0);
        assert_eq!(round_byte_fp_long(254.9), 254);
        assert_eq!(round_short_fp_long(1.9), 1);
        assert_eq!(round_short_fp_long(32767.9), 32767);
    }

    /// double 版も f32 版と同じく切り捨て・クランプ。
    #[test]
    fn double_round_helpers_truncate_and_clamp() {
        assert_eq!(round_byte_fp_double(1.9), 1);
        assert_eq!(round_byte_fp_double(-5.0), 0);
        assert_eq!(round_byte_fp_double(1000.0), MAX_CHANNEL8 as u8);
        assert_eq!(round_short_fp_double(1.9), 1);
        assert_eq!(round_short_fp_double(1.0e9), MAX_CHANNEL16 as u16);
    }

    #[test]
    fn round_helpers_clamp_to_the_channel_range() {
        assert_eq!(round_byte_fp_long(-5.0), 0);
        assert_eq!(round_byte_fp_long(300.0), MAX_CHANNEL8 as u8);
        assert_eq!(round_short_fp_long(-1.0), 0);
        assert_eq!(round_short_fp_long(1.0e9), MAX_CHANNEL16 as u16);
        assert_eq!(round_byte_long(-1), 0);
        assert_eq!(round_byte_long(999), MAX_CHANNEL8 as u8);
        assert_eq!(round_short(-1), 0);
        assert_eq!(round_short(1 << 20), MAX_CHANNEL16 as u16);
    }

    /// 元の RoundFpShort は 0..32 にクランプする (1.0 ではない)。
    #[test]
    fn round_fp_short_clamps_to_32() {
        assert_eq!(round_fp_short(-1.0), 0.0);
        assert_eq!(round_fp_short(5.5), 5.5);
        assert_eq!(round_fp_short(100.0), 32.0);
    }
}
