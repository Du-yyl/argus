use crate::tuples::Pair;
use crate::utils::exif_utils::gps_util::GpsUtil;
use crate::utils::exif_utils::value::ValueType;
use crate::utils::json_util::JsonUtil;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::format;

#[derive(Debug, Clone)]
pub struct Tag {
    /// 原始数据保存
    pub entries: Vec<(String, String)>,
    /// 数据映射
    pub entry_map: HashMap<String, String>,
}

/// 图像的 exif 信息对象
pub struct ImgExif {
    /// 相机制造商
    make: String,
    /// 相机型号
    model: String,
    /// 软件版本
    software: String,
    /// 曝光时间
    exposure_time: String,
    /// 闪光灯
    flash: String,
    /// 光圈
    f_number: String,
    /// ISO
    iso: String,
    /// exif 信息版本
    // exif_version:String,
    /// 创建日期
    date_time_original: String,
    /// 时区（+8）
    offset_time: u32,
    /// 最大光圈值
    max_aperture_value: String,
    /// 焦距
    focal_length: String,
    /// 宽度
    image_width: String,
    /// 长度
    image_height: String,
    /// gps 信息
    gps_info: String,
    /// 曝光程序
    exposure_program: String,
    /// 测光模式
    metering_mode: String,
    /// 作者（艺术家）
    artist: String,
}

impl Tag {
    pub fn parse(mut self, info: &str) -> Self {
        for line in info.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value.trim().to_string();
                self.entry_map.insert(key.clone(), value.clone());
                self.entries.push((key, value));
            }
        }
        self
    }

    pub fn get(&self, key: &str) -> Option<Cow<String>> {
        self.entry_map.get(key).map(|v| Cow::Borrowed(v))
    }

    /// 打包数据
    pub fn pack_tags(&self) -> anyhow::Result<String> {
        let mut res: Vec<Pair<String, String>> = Vec::new();
        ExifToolDesc::EXIF_INFOS_FRONT.map(|info| {
            let ans = self.get(info.exif_tool_desc);
            // 如果数据有值
            if ans.is_some() {
                res.push(Pair {
                    first: info.dis.to_string(),
                    second: ans.unwrap().to_string(),
                });
            }
        });
        JsonUtil::stringify(&res)
    }

    /// 打包为前端展示字段
    pub fn pack_front_tags(&self) -> anyhow::Result<String> {
        let mut res: Vec<Pair<String, String>> = Vec::new();

        // 使用一个辅助函数处理字段的封装
        let mut add_tag = |desc: &ExifInfo, field_name: &str| {
            self.get(desc.exif_tool_desc).map(|x| {
                res.push(Pair {
                    first: field_name.to_string(),
                    second: x.to_string(),
                });
            });
        };

        // 封装通用的字段添加逻辑
        add_tag(&ExifToolDesc::MAKE, ExifToolDesc::MAKE.dis);
        add_tag(&ExifToolDesc::MODEL, ExifToolDesc::MODEL.dis);
        add_tag(&ExifToolDesc::SOFTWARE, ExifToolDesc::SOFTWARE.dis);
        add_tag(&ExifToolDesc::ARTIST, ExifToolDesc::ARTIST.dis);
        add_tag(&ExifToolDesc::FLASH, ExifToolDesc::FLASH.dis);
        add_tag(&ExifToolDesc::FOCAL_LENGTH, ExifToolDesc::FOCAL_LENGTH.dis);
        add_tag(
            &ExifToolDesc::EXPOSURE_TIME,
            ExifToolDesc::EXPOSURE_TIME.dis,
        );
        add_tag(&ExifToolDesc::F_NUMBER, ExifToolDesc::F_NUMBER.dis);
        add_tag(&ExifToolDesc::ISO, ExifToolDesc::ISO.dis);
        add_tag(
            &ExifToolDesc::EXPOSURE_PROGRAM,
            ExifToolDesc::EXPOSURE_PROGRAM.dis,
        );
        add_tag(
            &ExifToolDesc::METERING_MODE,
            ExifToolDesc::METERING_MODE.dis,
        );

        // 焦距和等效焦距处理
        if let Some(focal_length) = self.get(ExifToolDesc::FOCAL_LENGTH.exif_tool_desc) {
            let mm35 = self
                .get(ExifToolDesc::FOCAL_LENGTH_IN_35MM_FORMAT.exif_tool_desc)
                .unwrap_or_default();
            let ans = if mm35.is_empty() {
                focal_length.to_string()
            } else {
                format!("{}, 等效焦距: {}", focal_length, mm35)
            };

            res.push(Pair {
                first: ExifToolDesc::FOCAL_LENGTH.dis.to_string(),
                second: ans,
            });
        }

        // GPS 信息拼接
        let gps_info = self.parse_gps_tags().unwrap_or_else(|err| String::from(""));
        res.push(Pair {
            first: "GPS 信息".to_string(),
            second: gps_info,
        });

        JsonUtil::stringify(&res)
    }

    /// 打包为对象
    pub fn pack_object(&self)->ImgExif {
        let make:String;
        let model:String;
        let software:String;
        let exposure_time:String;
        let flash:String;
        let f_number:String;
        let iso:String;
        let date_time_original:String;
        let offset_time:String;
        let max_aperture_value:String;
        let focal_length:String;
        let image_width:String;
        let image_height:String;
        let gps_info:String;
        let exposure_program:String;
        let metering_mode:String;
        let artist:String;

        

        todo!()
    }

    /// 解析 gps 数据【获取 gps 数据，并根据有无转换为文字信息】
    pub fn parse_gps_tags(&self) -> anyhow::Result<String> {
        // 经度
        let longitude: String;
        // 维度
        let dimensions;
        // 海拔
        let altitude;

        let gps_latitude = self.get(ExifToolDesc::GPS_LATITUDE.exif_tool_desc);
        if gps_latitude.is_some() {
            let gc = gps_latitude.unwrap_or_default().to_string();
            let string = GpsUtil::resolve_coordinate(gc);

            let gps_latitude_ref = self.get(ExifToolDesc::GPS_LATITUDE_REF.exif_tool_desc);
            let gc_ref = if gps_latitude_ref.is_some() {
                GpsUtil::resolve_direction(gps_latitude_ref.unwrap().to_string())
            } else {
                String::from("")
            };

            longitude = format!("{} {}", gc_ref, string)
        } else {
            longitude = String::from("")
        }

        let gps_longitude = self.get(ExifToolDesc::GPS_LONGITUDE.exif_tool_desc);
        if gps_longitude.is_some() {
            let gc = gps_longitude.unwrap_or_default().to_string();
            let string = GpsUtil::resolve_coordinate(gc);

            let gps_latitude_ref = self.get(ExifToolDesc::GPS_LONGITUDE_REF.exif_tool_desc);
            let gc_ref = if gps_latitude_ref.is_some() {
                GpsUtil::resolve_direction(gps_latitude_ref.unwrap().to_string())
            } else {
                String::from("")
            };

            dimensions = format!("{} {}", gc_ref, string)
        } else {
            dimensions = String::from("")
        }

        let gps_altitude = self
            .get(ExifToolDesc::GPS_ALTITUDE.exif_tool_desc)
            .unwrap_or_default()
            .to_string();
        altitude = GpsUtil::resolve_altitude(gps_altitude);

        Ok(format!("{},{},{}", longitude, dimensions, altitude))
    }

    pub fn new() -> Self {
        Tag {
            entries: Vec::new(),
            entry_map: HashMap::new(),
        }
    }
}

pub struct ExifToolDesc {}

impl ExifToolDesc {
    pub const MAKE: ExifInfo = ExifInfo {
        dis: "相机制造商",
        exif_tool_desc: "Make",
        value_type: ValueType::String,
    };
    pub const MODEL: ExifInfo = ExifInfo {
        dis: "相机型号",
        exif_tool_desc: "Camera Model Name",
        value_type: ValueType::String,
    };
    pub const SOFTWARE: ExifInfo = ExifInfo {
        dis: "软件",
        exif_tool_desc: "Software",
        value_type: ValueType::String,
    };
    pub const EXPOSURE_TIME: ExifInfo = ExifInfo {
        dis: "快门速度",
        exif_tool_desc: "Exposure Time",
        value_type: ValueType::String,
    };
    pub const F_NUMBER: ExifInfo = ExifInfo {
        dis: "光圈数",
        exif_tool_desc: "F Number",
        value_type: ValueType::String,
    };
    pub const ISO: ExifInfo = ExifInfo {
        dis: "ISO 感光度",
        exif_tool_desc: "ISO",
        value_type: ValueType::String,
    };
    pub const EXIF_VERSION: ExifInfo = ExifInfo {
        dis: "Exif 版本",
        exif_tool_desc: "Exif Version",
        value_type: ValueType::String,
    };
    pub const DATE_TIME_ORIGINAL: ExifInfo = ExifInfo {
        dis: "拍摄时间",
        exif_tool_desc: "Date/Time Original",
        value_type: ValueType::String,
    };
    pub const OFFSET_TIME: ExifInfo = ExifInfo {
        dis: "时区",
        exif_tool_desc: "Offset Time",
        value_type: ValueType::String,
    };
    pub const MAX_APERTURE_VALUE: ExifInfo = ExifInfo {
        dis: "最大光圈",
        exif_tool_desc: "Max Aperture Value",
        value_type: ValueType::String,
    };
    pub const FOCAL_LENGTH: ExifInfo = ExifInfo {
        dis: "焦距",
        exif_tool_desc: "Focal Length",
        value_type: ValueType::String,
    };
    pub const FOCAL_LENGTH_IN_35MM_FORMAT: ExifInfo = ExifInfo {
        dis: "等效焦距",
        exif_tool_desc: "Focal Length In 35mm Format",
        value_type: ValueType::String,
    };
    pub const IMAGE_WIDTH: ExifInfo = ExifInfo {
        dis: "图像宽度",
        exif_tool_desc: "Image Width",
        value_type: ValueType::String,
    };
    pub const IMAGE_HEIGHT: ExifInfo = ExifInfo {
        dis: "图像长度",
        exif_tool_desc: "Image Height",
        value_type: ValueType::String,
    };
    pub const GPS_LATITUDE_REF: ExifInfo = ExifInfo {
        dis: "GPS 纬度参考",
        exif_tool_desc: "GPS Latitude Ref",
        value_type: ValueType::String,
    };
    pub const GPS_LONGITUDE_REF: ExifInfo = ExifInfo {
        dis: "GPS 经度参考",
        exif_tool_desc: "GPS Longitude Ref",
        value_type: ValueType::String,
    };
    pub const GPS_LATITUDE: ExifInfo = ExifInfo {
        dis: "GPS 纬度",
        exif_tool_desc: "GPS Latitude",
        value_type: ValueType::String,
    };
    pub const GPS_LONGITUDE: ExifInfo = ExifInfo {
        dis: "GPS 经度",
        exif_tool_desc: "GPS Longitude",
        value_type: ValueType::String,
    };
    pub const GPS_ALTITUDE: ExifInfo = ExifInfo {
        dis: "GPS 海拔",
        exif_tool_desc: "GPS Altitude",
        value_type: ValueType::String,
    };
    pub const EXPOSURE_PROGRAM: ExifInfo = ExifInfo {
        dis: "曝光程序",
        exif_tool_desc: "Exposure Program",
        value_type: ValueType::String,
    };
    pub const METERING_MODE: ExifInfo = ExifInfo {
        dis: "测光模式",
        exif_tool_desc: "Metering Mode",
        value_type: ValueType::String,
    };
    pub const FLASH: ExifInfo = ExifInfo {
        dis: "闪光灯",
        exif_tool_desc: "Flash",
        value_type: ValueType::String,
    };
    pub const ARTIST: ExifInfo = ExifInfo {
        dis: "艺术家",
        exif_tool_desc: "Artist",
        value_type: ValueType::String,
    };

    pub const EXIF_INFOS: [&'static ExifInfo; 23] = [
        &Self::MAKE,
        &Self::MODEL,
        &Self::SOFTWARE,
        &Self::EXPOSURE_TIME,
        &Self::F_NUMBER,
        &Self::ISO,
        &Self::EXIF_VERSION,
        &Self::DATE_TIME_ORIGINAL,
        &Self::OFFSET_TIME,
        &Self::MAX_APERTURE_VALUE,
        &Self::FOCAL_LENGTH,
        &Self::FOCAL_LENGTH_IN_35MM_FORMAT,
        &Self::IMAGE_WIDTH,
        &Self::IMAGE_HEIGHT,
        &Self::GPS_LATITUDE_REF,
        &Self::GPS_LONGITUDE_REF,
        &Self::GPS_LATITUDE,
        &Self::GPS_LONGITUDE,
        &Self::GPS_ALTITUDE,
        &Self::EXPOSURE_PROGRAM,
        &Self::METERING_MODE,
        &Self::FLASH,
        &Self::ARTIST,
    ];
    /// 前端展示的数据
    pub const EXIF_INFOS_FRONT: [&'static ExifInfo; 23] = [
        &Self::MAKE,
        &Self::MODEL,
        &Self::SOFTWARE,
        &Self::EXPOSURE_TIME,
        &Self::F_NUMBER,
        &Self::ISO,
        &Self::EXIF_VERSION,
        &Self::DATE_TIME_ORIGINAL,
        &Self::OFFSET_TIME,
        &Self::MAX_APERTURE_VALUE,
        &Self::FOCAL_LENGTH,
        &Self::FOCAL_LENGTH_IN_35MM_FORMAT,
        &Self::IMAGE_WIDTH,
        &Self::IMAGE_HEIGHT,
        &Self::GPS_LATITUDE_REF,
        &Self::GPS_LONGITUDE_REF,
        &Self::GPS_LATITUDE,
        &Self::GPS_LONGITUDE,
        &Self::GPS_ALTITUDE,
        &Self::EXPOSURE_PROGRAM,
        &Self::METERING_MODE,
        &Self::FLASH,
        &Self::ARTIST,
    ];
}

#[derive(Clone, Debug)]
pub struct ExifInfo {
    pub dis: &'static str,
    /// exiftool 的文字描述（匹配数据）
    pub exif_tool_desc: &'static str,
    /// 数据类型
    pub value_type: ValueType,
}

/*
现在的目的是实现结构体的定义和现有数据的匹配，数据的解析等功能暂不考虑
*/
// 名称、描述、类型、默认值
// 生成标签常量
// macro_rules! generate_tag_constants {
//     (
//         $(
//             // 捕获传递的属性【可空】
//             // $(#[$attr:meta])*
//             ($name:ident ,$value:expr,$defval:expr, $desc:expr,$ExifToolDesc:expr, $type:ty)
//         )+
//
//     ) => (
//         struct ExifTag {
//             pub const $name: Tag = Tag($ctx, $num);
//         }
//
//     );
// }
//
// generate_tag_constants!();
