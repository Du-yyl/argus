use std::collections::HashSet;
use crate::errors::SqlError;
use crate::models::basic_setting::BasicSetting;
use crate::storage;
use crate::storage::connection::establish_connection;
use crate::utils::json_util::JsonUtil;
use serde_json::{self, Value, Error};

pub fn get_basic_setting() -> Result<BasicSetting, SqlError> {
    let mut conn = establish_connection();
    storage::basic_setting::get_basic_setting(&mut conn)
}

/// 更新图像地址
pub fn update_basic_setting_img_path(img_paths: String) -> Result<(), SqlError> {
    let mut conn = establish_connection();
    let mut basic_setting = get_basic_setting()?;
    let imgs: HashSet<String> = JsonUtil::from_json(&*img_paths).expect("数据转换失败!");
    let result = JsonUtil::stringify(&imgs).expect("数据序列化失败！");
    log::info!("更新后的地址列表: {}",result);
    basic_setting.img_paths = img_paths;
    storage::basic_setting::update_basic_setting(&mut conn, &mut basic_setting)
}