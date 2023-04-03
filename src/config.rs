use envconfig::Envconfig;

#[derive(Envconfig, Clone)]
pub struct Config {
    #[envconfig(from = "HOST_NAME")]
    pub host_name: String,

    #[envconfig(from = "REDIS_HOST_NAME")]
    pub redis_host_name: String,

    #[envconfig(from = "REDIS_PORT")]
    pub redis_port: u16,

    #[envconfig(from = "REDIS_DB")]
    pub redis_db: u16,

    #[envconfig(from = "HTTP_PORT")]
    pub http_port: u16,

    #[envconfig(from = "STATIC_PATH")]
    pub static_path: String,

    #[envconfig(from = "HCAPTCHA_SECRET")]
    pub hcaptcha_secret: String,
}
