use crate::utils::exif_utils::tag::{ExifToolDesc, Tags};
use crate::utils::exif_utils::value::ValueType::Gps;
use anyhow::{anyhow, Result};
use diesel::dsl::min;
use regex::Regex;
use rusqlite::ffi::fts5_api;
use std::cell::BorrowError;
use std::fmt;
use tokio::time::sleep;

/// exif 中的 gps 信息
#[derive(Default, Clone, Debug)]
pub struct GpsInfo {
    /// 纬度
    pub latitude_ref: Option<Direction>,
    /// 纬度
    pub latitude: Option<DMS>,

    /// 经度
    pub longitude_ref: Option<Direction>,
    /// 经度
    pub longitude: Option<DMS>,

    /// 海拔
    pub altitude_ref: Option<SeaLevel>,
    /// 海拔
    pub altitude: Option<f64>,

    /// 速度单位【不支持速度】
    /// - K: kilometers per hour
    /// - M: miles per hour
    /// - N: knots
    // pub speed_ref: Option<char>,
    // pub speed: Option<URational>,

    /// 遇到错误时继续
    continue_on_error: bool,
}

impl GpsInfo {
    /// 解析 gps 信息【把 tags 信息传入，进行 gps 解析】
    pub fn parse(tags: &Tags, continue_on_error: bool) -> Result<GpsInfo> {
        if tags.is_empty() {
            return Ok(GpsInfo::default());
        }
        let latitude_ref: Option<Direction>;
        let latitude: Option<DMS>;

        let longitude_ref: Option<Direction>;
        let longitude: Option<DMS>;

        let altitude_ref: Option<SeaLevel> = Some(SeaLevel::AboveSeaLevel);
        let altitude: Option<f64>;

        // 经度
        latitude_ref = if let Some(x) = tags.get(ExifToolDesc::GPS_LATITUDE_REF.exif_tool_desc) {
            Direction::from_str(x.as_str())
        } else {
            None
        };
        latitude = if let Some(x) = tags.get(ExifToolDesc::GPS_LATITUDE.exif_tool_desc) {
            DMS::parse_with_exiftool(x.as_str())
        } else {
            None
        };

        // 纬度
        longitude_ref = if let Some(x) = tags.get(ExifToolDesc::GPS_LONGITUDE_REF.exif_tool_desc) {
            Direction::from_str(x.as_str())
        } else {
            None
        };
        longitude = if let Some(x) = tags.get(ExifToolDesc::GPS_LONGITUDE.exif_tool_desc) {
            DMS::parse_with_exiftool(x.as_str())
        } else {
            None
        };

        // 海拔
        altitude = if let Some(x) = tags.get(ExifToolDesc::GPS_LONGITUDE.exif_tool_desc) {
            let result = SeaLevel::parse_with_exiftool(x.as_str());
            if continue_on_error {
                if result.is_err() {
                    None
                } else {
                    Some(result.unwrap_or_default())
                }
            } else {
                Some(result?)
            }
        } else {
            None
        };
        Ok(GpsInfo::new(
            latitude_ref,
            latitude,
            longitude_ref,
            longitude,
            altitude_ref,
            altitude,
        ))
    }

    pub fn new(
        latitude_ref: Option<Direction>,
        latitude: Option<DMS>,
        longitude_ref: Option<Direction>,
        longitude: Option<DMS>,
        altitude_ref: Option<SeaLevel>,
        altitude: Option<f64>,
    ) -> Self {
        Self {
            latitude_ref,
            latitude,
            longitude_ref,
            longitude,
            altitude_ref,
            altitude,
            continue_on_error: true,
        }
    }
}

/// 方向
#[derive(Default, Clone, Debug)]
pub enum Direction {
    #[default]
    South,
    North,
    West,
    East,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Direction::South => "S",
            Direction::North => "N",
            Direction::West => "W",
            Direction::East => "E",
        };
        write!(f, "{}", s)
    }
}

impl Direction {
    // 从字符串转换为 Direction
    pub fn from_str(s: &str) -> Option<Direction> {
        let s = s.trim().to_lowercase();

        match s.as_str() {
            "south" | "s" => Some(Direction::South),
            "north" | "n" => Some(Direction::North),
            "west" | "w" => Some(Direction::West),
            "east" | "e" => Some(Direction::East),
            _ => None, // 无匹配项
        }
    }
}

/// 表示度、分、秒
#[derive(Clone, Debug, Default)]
pub struct DMS {
    pub degrees: i32, // 度（int）
    pub minutes: i32, // 分（int）
    pub seconds: f64, // 秒（float）
}

impl DMS {
    pub fn new(degrees: i32, minutes: i32, seconds: f64) -> Self {
        DMS {
            degrees,
            minutes,
            seconds,
        }
    }

    /// 解析度分秒数据【只针对 exiftool 数据】
    pub fn parse_with_exiftool(dms: &str) -> Option<DMS> {
        // 匹配度数、分度、秒度和方向
        let re = Regex::new(r"(\d+) deg (\d+)' (\d+\.\d+)").unwrap();
        let x: Vec<_> = dms
            .split("\"")
            .map(str::trim) // 去除每部分的前后空白
            .collect();
        // 使用正则表达式进行匹配
        if let Some(caps) = re.captures(dms) {
            // 尝试解析度、分和秒，如果解析失败则返回 None
            let degrees: i32 = caps[1].parse().ok()?;
            let minutes: i32 = caps[2].parse().ok()?;
            let seconds: f64 = caps[3].parse().ok()?;

            // 返回 DMS 对象
            Some(DMS {
                degrees,
                minutes,
                seconds,
            })
        } else {
            // 如果没有匹配到，则返回 None
            None
        }
    }
}

/// 海平面信息
#[derive(Default, Clone, Debug)]
pub enum SeaLevel {
    /// 海平面以上
    #[default]
    AboveSeaLevel,
    /// 海平面一下
    BelowSeaLevel,
}

impl SeaLevel {
    /// 解析海拔
    pub fn parse_with_exiftool(coordinate: &str) -> Result<f64> {
        let string = coordinate
            .replace(" m ", "m")
            .replace("Above Sea Level", "");
        string.parse()
    }
}

mod tests {
    use crate::utils::exif_utils::gps_util::{SeaLevel, DMS};

    #[test]
    fn test1() {
        let str = "114 deg 9' 56.09\" E";
        let string = DMS::parse_with_exiftool(&str);
        println!("{:?}", string)
    }

    #[test]
    fn test2() {
        let str = "6 m Above Sea Level";
        let string = SeaLevel::parse_with_exiftool(str);
        println!("{:?}", string)
    }
}
