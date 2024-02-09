use std::fmt::Display;

use nom::{
    bytes::complete::{tag, take_while},
    character::complete::{alphanumeric1, digit1, line_ending, oct_digit1, space1},
    error::ErrorKind,
    multi::separated_list1,
    sequence::{delimited, tuple},
    Err::Error,
    IResult,
};

use crate::command::args::Ssim;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct SsimFrameData {
    pub frame: u32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
    pub all: f32,
}

impl SsimFrameData {
    pub fn new() -> Self {
        SsimFrameData {
            frame: 0,
            y: 0.0,
            u: 0.0,
            v: 0.0,
            all: 0.0,
        }
    }

    pub fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        let (input, (_, frame, _, _, y, _, _, u, _, _, v, _, _, all, _, _)) = tuple((
            tag("n:"),
            parse_decimal,
            space1,
            tag("Y:"),
            parse_float,
            space1,
            tag("U:"),
            parse_float,
            space1,
            tag("V:"),
            parse_float,
            space1,
            tag("All:"),
            parse_float,
            space1,
            parse_db_float,
        ))(input)?;

        Ok((
            input,
            SsimFrameData {
                frame: std::str::from_utf8(frame).unwrap().parse().unwrap(),
                y: std::str::from_utf8(y).unwrap().parse().unwrap(),
                u: std::str::from_utf8(u).unwrap().parse().unwrap(),
                v: std::str::from_utf8(v).unwrap().parse().unwrap(),
                all: std::str::from_utf8(all).unwrap().parse().unwrap(),
            },
        ))
    }
}

impl Display for SsimFrameData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Y:{}  U:{}  V:{}  All:{}",
            self.y, self.u, self.v, self.all
        )
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct SsimData {
    frames: u32,
    // (mean, min, max, harmonic mean)
    y: (f32, f32, f32, f32),
    u: (f32, f32, f32, f32),
    v: (f32, f32, f32, f32),
    all: (f32, f32, f32, f32),
}

impl SsimData {
    pub fn new() -> Self {
        SsimData {
            frames: 0,
            y: (0.0, 1.0, 0.0, 0.0),
            u: (0.0, 1.0, 0.0, 0.0),
            v: (0.0, 1.0, 0.0, 0.0),
            all: (0.0, 1.0, 0.0, 0.0),
        }
    }

    pub fn from_vec(input: &Vec<SsimFrameData>) -> Self {
        let mut out = SsimData {
            frames: 0,
            y: (0.0, input[0].y.clone(), input[0].y.clone(), 0.0),
            u: (0.0, input[0].u.clone(), input[0].u.clone(), 0.0),
            v: (0.0, input[0].v.clone(), input[0].v.clone(), 0.0),
            all: (0.0, input[0].all.clone(), input[0].all.clone(), 0.0),
        };

        for val in input.iter() {
            out.frames += 1;
            out.y.0 += val.y;
            if val.y < out.y.1 {
                out.y.1 = val.y.clone();
            }
            if val.y > out.y.2 {
                out.y.2 = val.y.clone();
            }
            out.y.3 += 1.0 / val.y;
            out.u.0 += val.u;
            if val.u < out.u.1 {
                out.u.1 = val.u.clone();
            }
            if val.u > out.u.2 {
                out.u.2 = val.u.clone();
            }
            out.u.3 += 1.0 / val.u;
            out.v.0 += val.v;
            if val.v < out.v.1 {
                out.v.1 = val.v.clone();
            }
            if val.v > out.v.2 {
                out.v.2 = val.v.clone();
            }
            out.v.3 += 1.0 / val.v;
            out.all.0 += val.all;
            if val.all < out.all.1 {
                out.all.1 = val.all.clone();
            }
            if val.all > out.all.2 {
                out.all.2 = val.all.clone();
            }
            out.all.3 += 1.0 / val.all;
        }

        out.y.0 /= out.frames as f32;
        out.y.3 = out.frames as f32 / out.y.3;
        out.u.0 /= out.frames as f32;
        out.u.3 = out.frames as f32 / out.u.3;
        out.v.0 /= out.frames as f32;
        out.v.3 = out.frames as f32 / out.v.3;
        out.all.0 /= out.frames as f32;
        out.all.3 = out.frames as f32 / out.all.3;

        out
    }

    pub fn increment_frames(&mut self) {
        self.frames += 1;
    }

    pub fn frames(&self) -> u32 {
        self.frames.clone()
    }

    pub fn y(&self) -> f32 {
        self.y.0.clone()
    }

    pub fn y_min(&self) -> f32 {
        self.y.1.clone()
    }

    pub fn y_max(&self) -> f32 {
        self.y.2.clone()
    }

    pub fn y_harmmean(&self) -> f32 {
        self.y.3.clone()
    }

    pub fn u(&self) -> f32 {
        self.u.0.clone()
    }

    pub fn u_min(&self) -> f32 {
        self.u.1.clone()
    }

    pub fn u_max(&self) -> f32 {
        self.u.2.clone()
    }

    pub fn u_harmmean(&self) -> f32 {
        self.u.3.clone()
    }

    pub fn v(&self) -> f32 {
        self.v.0.clone()
    }

    pub fn v_min(&self) -> f32 {
        self.v.1.clone()
    }

    pub fn v_max(&self) -> f32 {
        self.v.2.clone()
    }

    pub fn v_harmmean(&self) -> f32 {
        self.v.3.clone()
    }

    pub fn all(&self) -> f32 {
        self.all.0.clone()
    }

    pub fn all_min(&self) -> f32 {
        self.all.1.clone()
    }

    pub fn all_max(&self) -> f32 {
        self.all.2.clone()
    }

    pub fn all_harmmean(&self) -> f32 {
        self.all.3.clone()
    }
}

impl Display for SsimData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "frames: {},\nY, U, V, All\nMean: {}, {}, {}, {}\nMin: {}, {}, {}, {}\nMax: {}, {}, {}, {}\nHarmonic Mean: {}, {}, {}, {}",
            self.frames(),
            self.y(),
            self.u(),
            self.v(),
            self.all(),
            self.y_min(),
            self.u_min(),
            self.v_min(),
            self.all_min(),
            self.y_max(),
            self.u_max(),
            self.v_max(),
            self.all_max(),
            self.y_harmmean(),
            self.u_harmmean(),
            self.v_harmmean(),
            self.all_harmmean(),
        )
    }
}

fn parse_decimal(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while(|c| c >= b'0' && c <= b'9')(input)
}

fn parse_float(input: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while(|c| c >= b'0' && c <= b'9' || c == b'.')(input)
}

fn parse_db_float(input: &[u8]) -> IResult<&[u8], &[u8]> {
    delimited(
        tag("("),
        take_while(|c| c >= b'0' && c <= b'9' || c == b'.' || c == b'i' || c == b'n' || c == b'f'),
        tag(")"),
    )(input)
}

pub fn parse_ssim_stdout_line(input: &[u8]) -> IResult<&[u8], SsimFrameData> {
    let (input, (_, _, _, _, _, y, _, _, _, _, u, _, _, _, _, v, _, _, _, _, all)) = tuple((
        tag("[Parsed_ssim_"),
        digit1,
        tag(" @ 0x"),
        oct_digit1,
        tag("] SSIM Y:"),
        parse_float,
        space1,
        parse_db_float,
        space1,
        tag("U:"),
        parse_float,
        space1,
        parse_db_float,
        space1,
        tag("V:"),
        parse_float,
        space1,
        parse_db_float,
        space1,
        tag("All:"),
        parse_float,
    ))(input)?;

    Ok((
        input,
        SsimFrameData {
            frame: 0,
            y: std::str::from_utf8(y).unwrap().parse().unwrap(),
            u: std::str::from_utf8(u).unwrap().parse().unwrap(),
            v: std::str::from_utf8(v).unwrap().parse().unwrap(),
            all: std::str::from_utf8(all).unwrap().parse().unwrap(),
        },
    ))
}

pub fn parse_input(s: &[u8]) -> Vec<SsimFrameData> {
    let (remaining_input, lines) = separated_list1(line_ending, SsimFrameData::parse)(s).unwrap();
    lines
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_ssimdata_from_vec() {
        let vals = vec![
            SsimFrameData {
                frame: 1,
                y: 0.973668,
                u: 0.996946,
                v: 0.997587,
                all: 0.981534,
            },
            SsimFrameData {
                frame: 2,
                y: 0.957887,
                u: 0.954501,
                v: 0.971946,
                all: 0.959666,
            },
            SsimFrameData {
                frame: 3,
                y: 0.968484,
                u: 0.970053,
                v: 0.975606,
                all: 0.969932,
            },
            SsimFrameData {
                frame: 4,
                y: 0.933726,
                u: 0.934107,
                v: 0.971884,
                all: 0.940149,
            },
            SsimFrameData {
                frame: 5,
                y: 0.971588,
                u: 0.973521,
                v: 0.979230,
                all: 0.973184,
            },
            SsimFrameData {
                frame: 6,
                y: 0.935141,
                u: 0.939039,
                v: 0.968786,
                all: 0.941398,
            },
        ];
        let exemplar = SsimData {
            frames: 6,
            y: (0.956749, 0.933726, 0.973668, 0.95646054),
            u: (0.9613612, 0.934107, 0.996946, 0.9608823),
            v: (0.97750646, 0.968786, 0.997587, 0.9774142),
            all: (0.96097726, 0.940149, 0.981534, 0.9607211),
        };

        assert_eq!(SsimData::from_vec(&vals), exemplar);
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            parse_decimal("123 ".as_bytes()),
            Ok((" ".as_bytes(), ("123".as_bytes())))
        );
        assert_eq!(
            parse_decimal("  123 ".as_bytes()),
            Ok(("  123 ".as_bytes(), ("".as_bytes())))
        );
        assert_eq!(
            parse_decimal("    123    ".as_bytes()),
            Ok(("    123    ".as_bytes(), ("".as_bytes())))
        );
        assert_eq!(
            parse_decimal("123    ".as_bytes()),
            Ok(("    ".as_bytes(), ("123".as_bytes())))
        );
        assert_eq!(
            parse_decimal("123.5 ".as_bytes()),
            Ok((".5 ".as_bytes(), ("123".as_bytes())))
        );
    }

    #[test]
    fn test_parse_float() {
        assert_eq!(
            parse_float("123 ".as_bytes()),
            Ok((" ".as_bytes(), ("123".as_bytes())))
        );
        assert_eq!(
            parse_float("  123 ".as_bytes()),
            Ok(("  123 ".as_bytes(), ("".as_bytes())))
        );
        assert_eq!(
            parse_float("    123    ".as_bytes()),
            Ok(("    123    ".as_bytes(), ("".as_bytes())))
        );
        assert_eq!(
            parse_float("123    ".as_bytes()),
            Ok(("    ".as_bytes(), ("123".as_bytes())))
        );
        assert_eq!(
            parse_float("123.5 ".as_bytes()),
            Ok((" ".as_bytes(), ("123.5".as_bytes())))
        );
    }

    #[test]
    fn test_parse_db_float() {
        assert_eq!(
            parse_db_float("(123.5) ".as_bytes()),
            Ok((" ".as_bytes(), ("123.5".as_bytes())))
        );
        assert_ne!(
            parse_db_float("  (123.5) ".as_bytes()),
            Ok(("   ".as_bytes(), ("123.5".as_bytes())))
        );
        assert_ne!(
            parse_db_float("    (123.5)    ".as_bytes()),
            Ok(("        ".as_bytes(), ("123.5".as_bytes())))
        );
        assert_eq!(
            parse_db_float("(123.5)    ".as_bytes()),
            Ok(("    ".as_bytes(), ("123.5".as_bytes())))
        );
        assert_eq!(
            parse_db_float("(123.5) ".as_bytes()),
            Ok((" ".as_bytes(), ("123.5".as_bytes())))
        );
    }

    #[test]
    fn test_parse_ssim_line() {
        assert_eq!(
            SsimFrameData::parse(
                "n:5 Y:1.000000 U:1.000000 V:1.000000 All:1.000000 (inf)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 5,
                    y: 1.0,
                    u: 1.0,
                    v: 1.0,
                    all: 1.0,
                }
            ))
        );
        assert_eq!(
            SsimFrameData::parse(
                "n:1 Y:0.973668 U:0.996946 V:0.997587 All:0.981534 (17.336255)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 1,
                    y: 0.973668,
                    u: 0.996946,
                    v: 0.997587,
                    all: 0.981534,
                }
            ))
        );
        assert_eq!(
            SsimFrameData::parse(
                "n:19 Y:0.950286 U:0.956044 V:0.977148 All:0.955723 (13.538213)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 19,
                    y: 0.950286,
                    u: 0.956044,
                    v: 0.977148,
                    all: 0.955723,
                }
            ))
        );
        assert_eq!(
            SsimFrameData::parse(
                "n:403 Y:0.966575 U:0.990617 V:0.990982 All:0.974650 (15.960166)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 403,
                    y: 0.966575,
                    u: 0.990617,
                    v: 0.990982,
                    all: 0.974650,
                }
            ))
        );
        assert_eq!(
            SsimFrameData::parse(
                "n:4470 Y:0.955137 U:0.954697 V:0.979000 All:0.959041 (13.876490)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 4470,
                    y: 0.955137,
                    u: 0.954697,
                    v: 0.979000,
                    all: 0.959041,
                }
            ))
        );
        assert_eq!(
            SsimFrameData::parse(
                "n:14501 Y:0.966370 U:0.990612 V:0.990885 All:0.974497 (15.934016)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 14501,
                    y: 0.966370,
                    u: 0.990612,
                    v: 0.990885,
                    all: 0.974497,
                }
            ))
        );
        assert_eq!(
            SsimFrameData::parse(
                "n:200911 Y:0.936951 U:0.924235 V:0.969651 All:0.940282 (12.238937)".as_bytes()
            ),
            Ok((
                "".as_bytes(),
                SsimFrameData {
                    frame: 200911,
                    y: 0.936951,
                    u: 0.924235,
                    v: 0.969651,
                    all: 0.940282,
                }
            ))
        );
    }

    #[test]
    fn test_parse_ssim_stdout_line() {
        assert_eq!(
            parse_ssim_stdout_line("[Parsed_ssim_0 @ 0x600001174000] SSIM Y:0.960646 (14.050117) U:0.950001 (13.010355) V:0.983888 (17.928428) All:0.962745 (14.288204)".as_bytes()),
            Ok((
                " (14.288204)".as_bytes(),
                SsimFrameData {
                    frame: 0,
                    y: 0.960646,
                    u: 0.950001,
                    v: 0.983888,
                    all: 0.962745,
                }
            ))
        );
    }

    #[test]
    fn test_parse_ssim_lines_from_file_short() {
        let lines: Vec<String> = fs::read_to_string("src/command/ssim/sample/stats_short.log")
            .expect("failed to read input")
            .split("\n")
            .map(|s| s.to_string())
            .collect();
        let exemplar = vec![
            SsimFrameData {
                frame: 1,
                y: 0.973668,
                u: 0.996946,
                v: 0.997587,
                all: 0.981534,
            },
            SsimFrameData {
                frame: 2,
                y: 0.957887,
                u: 0.954501,
                v: 0.971946,
                all: 0.959666,
            },
            SsimFrameData {
                frame: 3,
                y: 0.968484,
                u: 0.970053,
                v: 0.975606,
                all: 0.969932,
            },
            SsimFrameData {
                frame: 4,
                y: 0.933726,
                u: 0.934107,
                v: 0.971884,
                all: 0.940149,
            },
            SsimFrameData {
                frame: 5,
                y: 0.971588,
                u: 0.973521,
                v: 0.979230,
                all: 0.973184,
            },
            SsimFrameData {
                frame: 6,
                y: 0.935141,
                u: 0.939039,
                v: 0.968786,
                all: 0.941398,
            },
        ];

        for (idx, line) in lines.iter().enumerate() {
            let line = line.as_str();
            if line == "" {
                break;
            }
            assert_eq!(
                SsimFrameData::parse(line.as_bytes()),
                Ok(("".as_bytes(), exemplar[idx].clone()))
            )
        }
    }

    #[test]
    fn test_parse_ssim_lines_from_file() {
        let byte_lines = fs::read("src/command/ssim/sample/ssim_stats.log").unwrap();
        let lines = std::str::from_utf8(&byte_lines).unwrap();
        let lines = parse_input(lines.as_bytes());
        let exemplar = SsimData {
            frames: 130125,
            y: (0.9285242, 0.832917, 1.0, 0.92777586),
            u: (0.92487913, 0.794935, 1.0, 0.92386687),
            v: (0.9736446, 0.90682, 1.0, 0.97354436),
            all: (0.9354518, 0.847191, 1.0, 0.9349192),
        };

        assert_eq!(SsimData::from_vec(&lines), exemplar);
    }
}
