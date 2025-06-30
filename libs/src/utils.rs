// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/FsLibrary/FsUtils.h

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

pub fn round_byte_fp_long(x: f32) -> u8 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > MAX_CHANNEL8 as f32 {
        temp = MAX_CHANNEL8 as f32;
    }
    temp.round() as u8
}

pub fn round_short_fp_long(x: f32) -> u16 {
    let mut temp = x;
    if temp < 0.0 {
        temp = 0.0;
    }
    if temp > MAX_CHANNEL16 as f32 {
        temp = MAX_CHANNEL16 as f32;
    }
    temp.round() as u16
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

pub fn conv_8_to_32(p: &PF_Pixel) -> PF_PixelFloat {
    //#define FS_CONVERT8TO32(A)      ((double)(long)((double)A*10000.0/(double)PF_MAX_CHAN8 + 0.5)/10000.0)
    PF_PixelFloat {
        //convert to 32-bit float
        alpha: (p.alpha as f32 * 10000.0 / MAX_CHANNEL8 as f32 + 0.5) / 10000.0,
        red: (p.red as f32 * 10000.0 / MAX_CHANNEL8 as f32 + 0.5) / 10000.0,
        green: (p.green as f32 * 10000.0 / MAX_CHANNEL8 as f32 + 0.5) / 10000.0,
        blue: (p.blue as f32 * 10000.0 / MAX_CHANNEL8 as f32 + 0.5) / 10000.0,
    }
}
