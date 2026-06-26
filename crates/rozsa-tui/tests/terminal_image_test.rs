use rozsa_tui::util::terminal_image::{
    calculate_cell_size, iterm2_transmit, kitty_transmit, parse_image_dimensions, ImageDimensions,
};

#[test]
fn parse_png_dimensions() {
    // 最小 PNG header（1x1 像素）
    let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    // IHDR chunk length
    data.extend_from_slice(&[0, 0, 0, 13]);
    // IHDR tag
    data.extend_from_slice(b"IHDR");
    // Width = 100, Height = 50
    data.extend_from_slice(&100u32.to_be_bytes());
    data.extend_from_slice(&50u32.to_be_bytes());

    let dims = parse_image_dimensions(&data).unwrap();
    assert_eq!(dims.width, 100);
    assert_eq!(dims.height, 50);
}

#[test]
fn parse_gif_dimensions() {
    let mut data = b"GIF89a".to_vec();
    data.extend_from_slice(&320u16.to_le_bytes()); // width
    data.extend_from_slice(&240u16.to_le_bytes()); // height
    data.extend_from_slice(&[0; 10]); // padding

    let dims = parse_image_dimensions(&data).unwrap();
    assert_eq!(dims.width, 320);
    assert_eq!(dims.height, 240);
}

#[test]
fn cell_size_calculation() {
    let dims = ImageDimensions { width: 800, height: 600 };
    let (cols, rows) = calculate_cell_size(dims, 80, 24);
    assert!(cols <= 80);
    assert!(rows <= 24);
    assert!(cols > 0);
    assert!(rows > 0);
}

#[test]
fn kitty_chunking() {
    let data = vec![0u8; 100];
    let result = kitty_transmit(&data, 10, 5);
    assert!(result.contains("\x1b_G"));
    assert!(result.contains("a=T"));
    assert!(result.contains("i="));
}

#[test]
fn iterm2_format() {
    let data = vec![0u8; 10];
    let result = iterm2_transmit(&data, 40);
    assert!(result.starts_with("\x1b]1337;File="));
    assert!(result.contains("width=40"));
    assert!(result.ends_with('\x07'));
}
