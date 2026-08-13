//! A window of rav's own.
//!
//! The frame is already a `Pixmap` on the CPU by the time it gets here, so this
//! is a blit target rather than a second renderer - which is the whole reason
//! the plan chose softbuffer over wgpu. What arrives is what the kitty surface
//! sends a terminal; only the delivery differs.

/// Repack a frame for softbuffer, resolving what is behind it.
///
/// softbuffer wants one `u32` per pixel as `0x00RRGGBB`, and
/// [`Canvas::to_rgba`](crate::render::Canvas::to_rgba) hands back four straight
/// bytes - demultiplied, alpha kept.
///
/// **The alpha has to be resolved here, and against black.** A terminal
/// composites rav's frame over whatever the cell behind it was, so the kitty
/// surface passes partial alpha on and lets it. A window has nothing behind it
/// at all, so an unresolved edge would arrive as whatever the last contents of
/// that buffer happened to be. Multiplying onto black is what "nothing behind
/// it" means, and it keeps an antialiased bar edge reading as a soft edge
/// rather than a bright one.
///
/// Writes into `into` rather than returning, because this runs on every frame
/// and a 2400x1440 window is 3.5 million pixels - a fresh allocation sixty
/// times a second is the trap the canvas already avoids being rebuilt for.
pub fn onto_black(rgba: &[u8], into: &mut Vec<u32>) {
    into.clear();
    into.reserve(rgba.len() / 4);
    into.extend(rgba.chunks_exact(4).map(|pixel| {
        let [red, green, blue, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        // `u16` for the product, then back down: `r * a` overflows a `u8` for
        // anything above the very darkest, and dividing by 255 rather than
        // shifting by 8 keeps full scale exactly full scale.
        let over_black = |channel: u8| (u16::from(channel) * u16::from(alpha) / 255) as u32;
        (over_black(red) << 16) | (over_black(green) << 8) | over_black(blue)
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(rgba: [u8; 4]) -> u32 {
        let mut out = Vec::new();
        onto_black(&rgba, &mut out);
        out[0]
    }

    #[test]
    fn an_opaque_pixel_keeps_its_colour() {
        assert_eq!(one([0xff, 0xff, 0xff, 0xff]), 0x00ff_ffff, "white");
        assert_eq!(one([0xff, 0x00, 0x00, 0xff]), 0x00ff_0000, "red");
        assert_eq!(one([0x00, 0xff, 0x00, 0xff]), 0x0000_ff00, "green");
        assert_eq!(one([0x00, 0x00, 0xff, 0xff]), 0x0000_00ff, "blue");
    }

    #[test]
    fn full_scale_survives_the_divide() {
        // The reason this is `/ 255` and not `>> 8`: shifting turns 255 into
        // 254, so a white bar in a window would be a shade under white and the
        // two pixel surfaces would not draw the same picture.
        assert_eq!(one([0xff, 0xff, 0xff, 0xff]) & 0xff, 0xff);
    }

    #[test]
    fn a_soft_edge_reads_as_dark_rather_than_as_bright() {
        // Half-covered white is mid grey, not white. Getting this backwards -
        // ignoring alpha - is what turns every antialiased bar edge into a
        // bright fringe.
        let half = one([0xff, 0xff, 0xff, 0x80]);
        assert_eq!(half, 0x0080_8080, "got {half:#08x}");
        assert_eq!(one([0xff, 0xff, 0xff, 0x00]), 0, "nothing there is black");
    }

    #[test]
    fn every_pixel_arrives_and_none_is_invented() {
        let frame: Vec<u8> = (0..64u8).collect();
        let mut out = Vec::new();
        onto_black(&frame, &mut out);
        assert_eq!(out.len(), 16, "16 pixels of 4 bytes");

        // Reused across frames, so it must not accumulate. A window that grew
        // its buffer by a screenful per frame would look fine and then run out
        // of memory somewhere in the second minute.
        onto_black(&frame, &mut out);
        assert_eq!(out.len(), 16, "the buffer was not cleared");
    }

    #[test]
    fn the_top_byte_is_left_alone() {
        // softbuffer reads `0x00RRGGBB` and ignores the top byte on some
        // platforms and not others. Leaving it zero is the portable answer.
        for alpha in [0x00, 0x7f, 0xff] {
            assert_eq!(one([0xff, 0xff, 0xff, alpha]) >> 24, 0, "alpha {alpha:#x}");
        }
    }
}
