use std::path::{Path, PathBuf};
use std::fs;
use std::io::Cursor;
use base64::Engine;
use std::io::Read;
use std::slice::RChunksExactMut;
use base64::{encode};
use base64::engine::general_purpose::STANDARD;
use crate::utils::base64_util::{base64_encode};
use crate::utils::file_util::{file_exists, read_binary_file};

/// 返回图像绝对路径
#[tauri::command]
pub fn get_image_absolute_path() -> String {
    let path = String::from("D:/argus/img/局部/5e9324ca411fa3f6.jpg");
    path
}

/// 检测是否有指定路径的访问权限
#[tauri::command]
pub fn check_directory_access(directory: String) -> Result<bool, String> {
    let path = Path::new(&directory);
    if path.is_dir() {
        match fs::read_dir(path) {
            Ok(_) => Ok(true),
            Err(err) => Err(err.to_string()),
        }
    } else {
        Err("Path is not a directory.".to_string())
    }
}

#[tauri::command]
pub fn read_image_as_base64(directory: String) -> Result<String, String> {
    // 检查文件是否存在
    if !file_exists(&directory) {
        return Err("File does not exist.".to_string());
    }

    // 读取照片
    let img = read_binary_file(&directory);
    match img {
        Ok(img) => {
            let result = base64_encode(img);
            Ok(result)
        }
        Err(err) => return Err(err.to_string()),
    }
}
