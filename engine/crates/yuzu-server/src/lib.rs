//! yuzu-server —— yuzu-web 资源后端(库)。
//!
//! 提供懒加载 XP3 资源 / 反编译剧本 / 解码图像的 axum 路由,
//! 以及嵌入式宿主可用的 `serve` / `api_routes`。
//! 独立二进制入口见 `main.rs`;yuzu-cli 的傻瓜模式(`yuzu <file.apk>`)内嵌本库。

pub mod api;
pub mod decode;
pub mod repo;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use repo::ArchiveRepo;

/// 同步阻塞启动服务器(内建 tokio 运行时),供命令行工具调用。
pub fn serve(data: impl Into<String>, web: impl Into<String>, addr: impl Into<String>) -> Result<()> {
    let data = data.into();
    let web = web.into();
    let addr = addr.into();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let state = Arc::new(api::AppState {
            repo: Arc::new(ArchiveRepo::new(&data)),
            web_dir: PathBuf::from(&web),
        });
        let app = api::router(state);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        println!("yuzu-server 就绪: http://{addr}  (data={data}  web={web})");
        axum::serve(listener, app).await?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
