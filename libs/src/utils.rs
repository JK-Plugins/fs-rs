// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/FsLibrary/FsUtils.h

use after_effects::{sys::{PF_Pixel, PF_Pixel16, PF_PixelFloat}, HALF_CHANNEL16, MAX_CHANNEL16, MAX_CHANNEL8};

pub fn abs<T>(x: T) -> T
where
    T: PartialOrd + std::ops::Neg<Output = T> + Copy + Default,
{
    if x >= T::default() { x } else { -x }
}

pub fn comp_pix8_lv(s: &PF_Pixel, d: &PF_Pixel, lv: u8) -> bool {
    (s.blue as i32 - d.blue as i32).abs() <= lv as i32 &&
    (s.green as i32 - d.green as i32).abs() <= lv as i32 &&
    (s.red as i32 - d.red as i32).abs() <= lv as i32
}

pub fn round_byte_fp_long(x: f32) -> u8 {
    let mut temp = x;
    if temp < 0.0 { temp = 0.0; }
    if temp > MAX_CHANNEL8 as f32 { temp = MAX_CHANNEL8 as f32; }
    temp.round() as u8
}

pub fn conv_16_to_8(p: &PF_Pixel16) -> PF_Pixel {
    PF_Pixel {
        alpha: round_byte_fp_long(((p.alpha as u32 * MAX_CHANNEL8 + HALF_CHANNEL16) / MAX_CHANNEL16) as f32),
        red: round_byte_fp_long(((p.red as u32 * MAX_CHANNEL8 + HALF_CHANNEL16) / MAX_CHANNEL16) as f32),
        green: round_byte_fp_long(((p.green as u32 * MAX_CHANNEL8 + HALF_CHANNEL16) / MAX_CHANNEL16) as f32),
        blue: round_byte_fp_long(((p.blue as u32 * MAX_CHANNEL8 + HALF_CHANNEL16) / MAX_CHANNEL16) as f32),
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
