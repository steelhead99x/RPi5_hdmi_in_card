use crate::error::{err, Result};

/// Packed RGB24 geometry. `stride` is the driver's line size, or the packed
/// row size when the driver reports a stride of zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RgbLayout {
    pub width: u32,
    pub height: u32,
    pub row: usize,
    pub stride: usize,
    pub packed: usize,
}

pub fn rgb24_layout(width: u32, height: u32, bytesperline: u32) -> Result<RgbLayout> {
    if width == 0 || height == 0 {
        return Err(err("The picture size is 0."));
    }
    let row = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(3))
        .ok_or_else(|| err("The picture is too large to pack."))?;
    let stride = if bytesperline == 0 {
        row
    } else {
        usize::try_from(bytesperline).map_err(|_| err("The picture is too large to pack."))?
    };
    if stride < row {
        return Err(err(format!(
            "The line stride is {stride} bytes, which is shorter than one RGB row of {row} bytes."
        )));
    }
    let rows = usize::try_from(height).map_err(|_| err("The picture is too large to pack."))?;
    let packed = row
        .checked_mul(rows)
        .ok_or_else(|| err("The picture is too large to pack."))?;
    Ok(RgbLayout {
        width,
        height,
        row,
        stride,
        packed,
    })
}

/// Copy one RGB24 frame, dropping driver line padding and any plane header.
pub fn pack_rgb24(buffer: &[u8], layout: &RgbLayout, data_offset: u32) -> Result<Vec<u8>> {
    if layout.height == 0 || layout.row == 0 {
        return Err(err("The picture size is 0."));
    }
    let start =
        usize::try_from(data_offset).map_err(|_| err("The plane data offset does not fit."))?;
    if start > buffer.len() {
        return Err(err(format!(
            "The plane data offset is {start} bytes, past the end of the {} byte buffer.",
            buffer.len()
        )));
    }
    let rows =
        usize::try_from(layout.height).map_err(|_| err("The picture is too large to pack."))?;
    let last = rows - 1;
    let need = last
        .checked_mul(layout.stride)
        .and_then(|value| value.checked_add(layout.row))
        .and_then(|value| value.checked_add(start))
        .ok_or_else(|| err("The picture is too large to pack."))?;
    if buffer.len() < need {
        return Err(err(format!(
            "The captured buffer is {} bytes. A {}x{} RGB frame with stride {} needs {need} bytes.",
            buffer.len(),
            layout.width,
            layout.height,
            layout.stride
        )));
    }
    let mut packed = Vec::with_capacity(layout.packed);
    for row_index in 0..rows {
        let from = start + row_index * layout.stride;
        packed.extend_from_slice(&buffer[from..from + layout.row]);
    }
    Ok(packed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_rows_are_copied_unchanged() {
        let layout = rgb24_layout(2, 1, 0).unwrap();
        let frame = [1, 2, 3, 4, 5, 6];
        assert_eq!(pack_rgb24(&frame, &layout, 0).unwrap(), frame);
        assert_eq!(layout.packed, 6);
    }

    #[test]
    fn line_padding_and_a_plane_header_are_removed() {
        let layout = rgb24_layout(2, 2, 8).unwrap();
        assert_eq!(layout.row, 6);
        assert_eq!(layout.stride, 8);
        assert_eq!(layout.packed, 12);
        let buffer = [
            0xAA, 0xAA, // header
            1, 2, 3, 4, 5, 6, 0xFF, 0xFF, // row 0 plus pad
            7, 8, 9, 10, 11, 12, 0xFF, 0xFF, // row 1 plus pad
        ];
        assert_eq!(
            pack_rgb24(&buffer, &layout, 2).unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        );
    }

    #[test]
    fn the_last_row_does_not_need_trailing_padding() {
        let layout = rgb24_layout(2, 2, 8).unwrap();
        let buffer = [1, 2, 3, 4, 5, 6, 0xFF, 0xFF, 7, 8, 9, 10, 11, 12];
        assert_eq!(
            pack_rgb24(&buffer, &layout, 0).unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        );
    }

    #[test]
    fn short_stride_short_buffer_and_empty_size_are_errors() {
        assert!(rgb24_layout(0, 10, 0).is_err());
        assert!(rgb24_layout(2, 2, 4)
            .unwrap_err()
            .to_string()
            .contains("shorter"));
        let layout = rgb24_layout(2, 2, 8).unwrap();
        let error = pack_rgb24(&[1, 2, 3, 4, 5, 6], &layout, 0).unwrap_err();
        assert!(error.to_string().contains("needs"), "{error}");
        let error = pack_rgb24(&[0, 1, 2, 3], &layout, 8).unwrap_err();
        assert!(error.to_string().contains("data offset"), "{error}");
    }
}
