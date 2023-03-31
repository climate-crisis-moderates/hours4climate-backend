use crate::config::Config;

pub fn create_client(config: &Config) -> redis::Client {
    let info = redis::ConnectionInfo {
        addr: redis::ConnectionAddr::Tcp(config.redis_host_name.clone(), config.redis_port),
        redis: redis::RedisConnectionInfo {
            db: config.redis_db as i64,
            username: None,
            password: None,
        },
    };

    redis::Client::open(info).unwrap()
}
