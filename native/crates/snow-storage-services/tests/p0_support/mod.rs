//! 每个用例独占一个文件数据库；不调用应用默认数据库路径。
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once};

use snow_storage_services::database::close_pooled_connections;
use uuid::Uuid;

static TEST_PROCESS: Mutex<()> = Mutex::new(());
static RESTORE_STD_PANIC_HOOK: Once = Once::new();

pub struct TestDb {
    directory: PathBuf,
    database: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl TestDb {
    pub fn new() -> Self {
        // snow-support 在进程启动时注册写个人 panic.log 的钩子。这里只在独立的
        // 测试二进制里恢复标准 stderr 钩子，失败时也不会写入用户的 Snow 日志。
        RESTORE_STD_PANIC_HOOK.call_once(|| drop(std::panic::take_hook()));
        // 连接池是进程全局的；只串行化同一测试二进制内使用该池的夹具。
        let guard = TEST_PROCESS.lock().unwrap_or_else(|err| err.into_inner());
        let directory = std::env::temp_dir().join(format!("snow-p0-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("独占创建合成库目录");
        let database = directory.join("fixture.sqlite");
        Self {
            directory,
            database,
            _guard: guard,
        }
    }

    pub fn path(&self) -> &Path {
        &self.database
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Windows 上先关闭空闲句柄，再清理本夹具创建的 SQLite/WAL/SHM 文件。
        close_pooled_connections();
        if let Err(error) = fs::remove_dir_all(&self.directory) {
            if std::thread::panicking() {
                eprintln!("测试失败后未能清理 {:?}: {error}", self.directory);
            } else {
                panic!("未能清理合成库 {:?}: {error}", self.directory);
            }
        }
    }
}
