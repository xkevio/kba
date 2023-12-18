pub mod lcd;
pub mod sprite;

/// Special Color Effect: Alpha Blending.
///
/// `I = eva * target_px_a + evb * target_px_b`.
pub fn blend(target_px_a: u16, target_px_b: u16, eva: u8, evb: u8) -> u16 {
    let eva_coeff = eva.clamp(0, 16);
    let evb_coeff = evb.clamp(0, 16);

    let r_a = (target_px_a & 0x1F) as u8;
    let g_a = ((target_px_a >> 5) & 0x1F) as u8;
    let b_a = ((target_px_a >> 10) & 0x1F) as u8;

    let r_b = (target_px_b & 0x1F) as u8;
    let g_b = ((target_px_b >> 5) & 0x1F) as u8;
    let b_b = ((target_px_b >> 10) & 0x1F) as u8;

    let r = (eva_coeff * r_a + evb_coeff * r_b).min(31) as u16;
    let g = (eva_coeff * g_a + evb_coeff * g_b).min(31) as u16;
    let b = (eva_coeff * b_a + evb_coeff * b_b).min(31) as u16;

    b << 10 | g << 5 | r
}

/// Special Color Effect: Increase/Decrease Brightness.
///
/// `MODE = true` -> Increase, else Decrease.
pub fn modify_brightness<const MODE: bool>(target_px_a: u16, evy: u8) -> u16 {
    let evy_coeff = evy.clamp(0, 16) as u16;

    let r_a = target_px_a & 0x1F;
    let g_a = (target_px_a >> 5) & 0x1F;
    let b_a = (target_px_a >> 10) & 0x1F;

    let [r, g, b] = [r_a, g_a, b_a].map(|c| match MODE {
        true => c + (31 - c) * evy_coeff,
        false => c - (c * evy_coeff),
    });

    b << 10 | g << 5 | r
}

/// Convert RGB555 color values to full 32 bit pixels.
pub fn rgb555_to_color(rgb: u16) -> u32 {
    let red = (rgb & 0x1F) as u8;
    let green = ((rgb >> 5) & 0x1F) as u8;
    let blue = ((rgb >> 10) & 0x1F) as u8;

    u32::from_be_bytes([
        (red << 3) | (red >> 2),
        (green << 3) | (green >> 2),
        (blue << 3) | (blue >> 2),
        255,
    ])
}
