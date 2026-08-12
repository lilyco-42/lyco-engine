//! yuzu-server —— yuzu-web 资源后端。
//!
//! 提供:静态 web 播放器 + 懒加载游戏资源 API(归档 / 反编译剧本 / 解码图像)。
//!
//! ```sh
//! yuzu-server --data realgame --web web --addr 0.0.0.0:8080
//! ```

mod api;
mod decode;
mod repo;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use repo::ArchiveRepo;

#[derive(Parser)]
#[command(
    name = "yuzu-server",
    version,
    about = "yuzu-web 资源后端:懒加载 XP3 / 反编译剧本 / 解码图像"
)]
struct Cli {
    /// 游戏数据目录(含 .xp3 归档)
    #[arg(long, default_value = "realgame")]
    data: String,
    /// web 静态目录(播放器页面)
    #[arg(long, default_value = "web")]
    web: String,
    /// 监听地址
    #[arg(long, default_value = "0.0.0.0:8080")]
    addr: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let state = Arc::new(api::AppState {
        repo: Arc::new(ArchiveRepo::new(cli.data.clone())),
        web_dir: PathBuf::from(cli.web.clone()),
    });
    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(&cli.addr).await?;
    println!(
        "yuzu-server 就绪: http://{}  (data={}  web={})",
        cli.addr, cli.data, cli.web
    );
    axum::serve(listener, app).await?;
    Ok(())
}
