use plotters::prelude::*;
use plotters::style::colors::full_palette::GREEN_A700;
use std::path::PathBuf;

use crate::command::vmaf;

pub fn plot(pts: Vec<(f32, f32)>, min: &f32, mean: &f32, filename: PathBuf) {
    let size = pts.len();
    let root = BitMapBackend::new(filename.to_str().unwrap(), (2000, 1000)).into_drawing_area();
    root.fill(&WHITE);
    let root = root.margin(10, 10, 10, 10);
    // After this point, we should be able to construct a chart context
    let mut chart = ChartBuilder::on(&root)
        // Set the size of the label region
        .x_label_area_size(100)
        .y_label_area_size(100)
        .margin(20)
        // Finally attach a coordinate on the drawing area and make a chart context
        .build_cartesian_2d(0f32..size as f32, 80f32..100f32)
        .unwrap();

    // Then we can draw a mesh
    chart
        .configure_mesh()
        // We can customize the maximum number of labels allowed for each axis
        .x_labels(5)
        .y_labels(5)
        // We can also change the format of the label text
        .x_label_formatter(&|x| format!("{:.0}", x))
        .y_label_formatter(&|x| format!("{:.1}", x))
        .x_label_style(("sans-serif", 30))
        .y_label_style(("sans-serif", 30))
        .x_desc("Frame")
        .y_desc("VMAF")
        .draw()
        .unwrap();
    chart
        .draw_series(LineSeries::new(
            pts,
            ShapeStyle {
                color: GREEN_A700.into(),
                filled: false,
                stroke_width: 2,
            },
        ))
        .unwrap()
        .label("VMAF")
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + 30, y)],
                ShapeStyle {
                    color: GREEN_A700.into(),
                    filled: false,
                    stroke_width: 5,
                },
            )
        });

    chart
        .draw_series(LineSeries::new(
            (0..size).map(|x| (x as f32, *min)),
            ShapeStyle {
                color: RED.into(),
                filled: false,
                stroke_width: 5,
            },
        ))
        .unwrap()
        .label("Min")
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + 30, y)],
                ShapeStyle {
                    color: RED.into(),
                    filled: false,
                    stroke_width: 5,
                },
            )
        });
    chart
        .draw_series(LineSeries::new(
            (0..size).map(|x| (x as f32, *mean)),
            ShapeStyle {
                color: BLUE.into(),
                filled: false,
                stroke_width: 5,
            },
        ))
        .unwrap()
        .label("Harmonic Mean")
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + 30, y)],
                ShapeStyle {
                    color: BLUE.into(),
                    filled: false,
                    stroke_width: 5,
                },
            )
        });

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::LowerRight)
        .border_style(&BLACK)
        .background_style(&WHITE)
        .label_font(("sans-serif", 40))
        .draw()
        .unwrap();
    root.present().unwrap();
}
