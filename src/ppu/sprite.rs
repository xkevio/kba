use proc_bitfield::ConvRaw;

pub struct Sprite {
    pub x: u16,
    pub y: u8,

    pub rot_scale: bool,
    pub double_or_disable: bool,

    pub obj_mode: ObjMode,
    pub mosaic: bool,
    pub bpp: bool,
    pub shape: ObjShape,

    pub rot_scale_param: u16,
    pub h_flip: bool,
    pub v_flip: bool,
    pub size: u8,

    pub tile_id: u16,
    pub prio: u8,
    pub pal_idx: u8,
}

#[derive(ConvRaw)]
pub enum ObjMode {
    Normal,
    SemiTransparent,
    Window,
    Prohibited,
}

#[derive(ConvRaw)]
pub enum ObjShape {
    Square,
    Horizontal,
    Vertical,
    Prohibited,
}

impl From<u64> for Sprite {
    fn from(value: u64) -> Self {
        let obj0 = value as u16;
        let obj1 = (value >> 16) as u16;
        let obj2 = (value >> 32) as u16;

        Self {
            x: obj1 & 0x1FF,
            y: obj0 as u8,

            rot_scale: obj0 & 0x100 != 0,
            double_or_disable: obj0 & (1 << 9) != 0,

            obj_mode: ObjMode::try_from((obj0 & 0x0C00) >> 10).unwrap(),
            mosaic: obj0 & (1 << 12) != 0,
            bpp: obj0 & (1 << 13) != 0,
            shape: ObjShape::try_from(obj0 >> 14).unwrap(),

            rot_scale_param: obj1 & 0x3E00,
            h_flip: obj1 & (1 << 12) != 0,
            v_flip: obj1 & (1 << 13) != 0,
            size: (obj1 >> 14) as u8,

            tile_id: obj2 & 0x3FF,
            prio: ((obj2 & 0x0C00) >> 10) as u8,
            pal_idx: (obj2 >> 12) as u8,
        }
    }
}
