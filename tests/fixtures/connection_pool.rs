use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

pub struct ConnectionPool {
    pool: Pool,
}

impl ConnectionPool {
    pub fn new(database_url: &str, max_size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let mut cfg = Config::new();
        cfg.url = Some(database_url.to_string());
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(max_size));

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

        Ok(Self { pool })
    }

    pub async fn get_connection(
        &self,
    ) -> Result<deadpool_postgres::Client, deadpool_postgres::PoolError> {
        self.pool.get().await
    }

    pub fn status(&self) -> PoolStatus {
        let status = self.pool.status();
        PoolStatus {
            available: status.available,
            size: status.size,
            max_size: status.max_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub available: usize,
    pub size: usize,
    pub max_size: usize,
}
