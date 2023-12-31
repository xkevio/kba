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

impl Sprite {
    pub fn collect_obj_ly(oam: &[u8], ly: u8) -> Vec<Sprite> {
        let mut sprites = Vec::new();

        // 6 bytes for the three OBJ attributes, extra byte for rotation parameters.
        for attributes in oam.chunks(8) {
            let attr = u64::from_le_bytes(attributes.try_into().unwrap());
            let sprite = Sprite::from(attr);

            // Treat y as signed with [-127, 128].
            // Won't fully work for affine double sprite size.
            let mut signed_start = sprite.y as i8;
            let signed_end = (signed_start as i16 + sprite.height() as i16) as i8;
            let wrapped_ly = ly as i8 as i16;

            // TODO: simplify wrapped range check!
            let contain = loop {
                if signed_start == signed_end {
                    break false;
                } else if signed_start as i16 == wrapped_ly {
                    break true;
                } else {
                    signed_start += 1;
                }
            };

            if contain {
                sprites.push(sprite);
            }
        }

        sprites
    }

    pub fn width(&self) -> u8 {
        use ObjShape::*;
        match (self.size, &self.shape) {
            (0, Square | Vertical) => 8,
            (0, Horizontal) => 16,
            (1, Square) => 16,
            (1, Horizontal) => 32,
            (1, Vertical) => 8,
            (2, Square | Horizontal) => 32,
            (2, Vertical) => 16,
            (3, Square | Horizontal) => 64,
            (3, Vertical) => 32,
            _ => unreachable!(),
        }
    }

    pub fn height(&self) -> u8 {
        use ObjShape::*;
        match (self.size, &self.shape) {
            (0, Square | Horizontal) => 8,
            (0, Vertical) => 16,
            (1, Square) => 16,
            (1, Horizontal) => 8,
            (1, Vertical) => 32,
            (2, Square | Vertical) => 32,
            (2, Horizontal) => 16,
            (3, Square | Vertical) => 64,
            (3, Horizontal) => 32,
            _ => unreachable!(),
        }
    }
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

            rot_scale_param: (obj1 & 0x3E00) >> 9,
            h_flip: obj1 & (1 << 12) != 0,
            v_flip: obj1 & (1 << 13) != 0,
            size: (obj1 >> 14) as u8,

            tile_id: obj2 & 0x3FF,
            prio: ((obj2 & 0x0C00) >> 10) as u8,
            pal_idx: (obj2 >> 12) as u8,
        }
    }
}
