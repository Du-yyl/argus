use crate::global_front_emit;
use crate::global_task_manager::BackgroundImageLoadingTaskManager;
use crate::storage::connection::establish_connection;
use crate::storage::photo_table::insert_photo;
use crate::structs::global_error_msg::{
    GlobalErrorMsg, GLOBAL_EMIT_APP_HANDLE, GLOBAL_EMIT_IS_INIT,
};
use crate::utils::img_util::ImageOperate;
use crate::utils::json_util::JsonUtil;
use crate::utils::task_util::task_h;
use anyhow::Result;
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, Mutex};
use crate::commands::file_command::FolderImage;
use crate::utils::file_util::{get_all_dir_img, get_all_subfolders};

#[tauri::command]
pub async fn add_photo_retrieve_task(
    global_task_manager: State<'_, Mutex<BackgroundImageLoadingTaskManager>>,
    tasks: Vec<String>,
) -> Result<String, String> {
    
    println!("add_task: {:?}", tasks);
    // 获取指定路径下所有的文件
    let mut result: Vec<String> = Vec::new();
    for x in tasks {
        let vec = get_all_subfolders(&x);
        // 使用并发处理文件夹
        for x in &vec {
            let display = x.display().to_string();

            // 获取所有照片
            let vec1 = get_all_dir_img(&display, Some(-1)); // 获取文件夹中的图像路径
            
            if !vec1.is_empty() {
                result.extend(vec1)
            }
        }   
    }
    // 添加任务
    for x in result {
        global_task_manager.lock().await.send_task(x).await;
    }
    Ok(String::from("完成"))
}

#[tauri::command]
pub async fn pause_task(
    global_task_manager: State<'_, Arc<Mutex<BackgroundImageLoadingTaskManager>>>,
) -> Result<String, String> {
    println!("pause_task");
    let manager = global_task_manager.lock();
    manager.await.pause().await;
    Ok(String::from("完成"))
}

#[tauri::command]
pub async fn resume_task(
    global_task_manager: State<'_, Arc<Mutex<BackgroundImageLoadingTaskManager>>>,
) -> Result<String, String> {
    println!("resume_task");
    let manager = global_task_manager.lock();
    manager.await.resume().await;
    Ok(String::from("完成"))
}

#[tauri::command]
pub fn emit_global_msg(app: AppHandle) {
    let mut is_init = GLOBAL_EMIT_IS_INIT.lock().unwrap();
    if is_init.clone() {
        return;
    }
    println!("前端 emit 初始化!");
    *is_init = true;

    let mut emit = GLOBAL_EMIT_APP_HANDLE.lock().unwrap();
    let (emit_tx, emit_rx) = mpsc::channel::<String>(100);
    *emit = Some(emit_tx);
    let f = move |info: String| {
        app.clone()
            .emit(global_front_emit::GLOBAL_ERROR_MSG_DISPLAY, info)
            .unwrap();
    };

    // 在一个新的线程中启动 Tokio 运行时
    thread::spawn(move || {
        // 创建 Tokio 运行时并运行异步任务
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            task_h(emit_rx, f).await;
        });
    });
}

#[tauri::command]
pub async fn global_msg_emit() -> Result<String, String> {
    let emit_option = {
        let emit = GLOBAL_EMIT_APP_HANDLE.lock().unwrap();
        emit.clone() // 移出作用域以释放锁
    };

    if let Some(x) = emit_option.as_ref() {
        let ms = GlobalErrorMsg {
            title: "标题".parse().unwrap(),
            msg: "内容".parse().unwrap(),
            duration: 5000,
            kind: crate::structs::global_error_msg::GlobalErrorMsgTypeEnum::Success,
        };
        let result = JsonUtil::stringify(&ms).expect("数据序列化失败!");
        let qqq = x.send(result).await;
        if qqq.is_err() {
            println!("发送失败！");
        }
    }
    return Ok(String::from("Ok"));
}
