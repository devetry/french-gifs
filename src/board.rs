use image::Rgb;
use image::{buffer::EnumeratePixels, Pixel};
use rpi_led_matrix::{LedColor, LedMatrix, LedMatrixOptions};

pub fn show_board<P: Pixel<Subpixel = u8>>(
    colors: EnumeratePixels<P>,
) -> Result<LedMatrix, &'static str> {
    struct ColorCoord {
        x: i32,
        y: i32,
        color: Rgb<u8>,
    }

    let color_list = colors
        .map(|c| ColorCoord {
            x: c.0 as i32,
            y: c.1 as i32,
            color: c.2.to_rgb(),
        })
        .collect::<Vec<_>>();

    let mut options = LedMatrixOptions::new();
    options.set_hardware_mapping("adafruit-hat-pwm");
    options.set_cols(64);
    options.set_rows(64);
    let matrix = LedMatrix::new(Some(options), None).unwrap();
    let mut canvas = matrix.offscreen_canvas();

    for item in &color_list {
        canvas.set(
            item.x,
            item.y,
            &LedColor {
                red: item.color[0],
                green: item.color[1],
                blue: item.color[2],
            },
        );
    }

    matrix.swap(canvas);

    Ok(matrix)
}
