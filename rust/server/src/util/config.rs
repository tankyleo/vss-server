use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Deserialize)]
struct Config {
	server_config: ServerConfig,
	jwt_auth_config: Option<JwtAuthConfig>,
	postgresql_config: Option<PostgreSQLConfig>,
}

#[derive(Deserialize)]
struct ServerConfig {
	bind_address: SocketAddr,
}

#[derive(Deserialize)]
struct JwtAuthConfig {
	rsa_pem: Option<String>,
}

const JWT_RSA_PEM_VAR: &str = "VSS_JWT_RSA_PEM";
const PSQL_USER_VAR: &str = "VSS_PSQL_USERNAME";
const PSQL_PASS_VAR: &str = "VSS_PSQL_PASSWORD";
const PSQL_ADDR_VAR: &str = "VSS_PSQL_ADDRESS";
const PSQL_DB_VAR: &str = "VSS_PSQL_DEFAULT_DB";
const PSQL_VSS_DB_VAR: &str = "VSS_PSQL_VSS_DB";
const PSQL_TLS_VAR: &str = "VSS_PSQL_TLS";
const PSQL_CERT_PEM_VAR: &str = "VSS_PSQL_CRT_PEM";

#[derive(Deserialize)]
struct PostgreSQLConfig {
	username: Option<String>,
	password: Option<String>,
	address: Option<SocketAddr>,
	default_database: Option<String>,
	vss_database: Option<String>,
	tls: Option<TlsConfig>,
}

#[derive(Deserialize)]
struct TlsConfig {
	crt_pem: Option<String>,
}

pub(crate) struct Configuration {
	pub(crate) bind_address: SocketAddr,
	pub(crate) rsa_pem: Option<String>,
	pub(crate) postgresql_prefix: String,
	pub(crate) default_db: String,
	pub(crate) vss_db: String,
	pub(crate) tls_config: Option<Option<String>>,
}

pub(crate) fn load_configuration(config_file_path: &str) -> Result<Configuration, String> {
	let config_file = std::fs::read_to_string(config_file_path)
		.map_err(|e| format!("Failed to read configuration file: {}", e))?;
	let Config { server_config: ServerConfig { bind_address }, jwt_auth_config, postgresql_config } =
		toml::from_str(&config_file)
			.map_err(|e| format!("Failed to parse configuration file: {}", e))?;

	macro_rules! read_env {
		($env_var:expr) => {
			match std::env::var($env_var) {
				Ok(env) => Some(env),
				Err(std::env::VarError::NotPresent) => None,
				Err(e) => {
					return Err(format!(
						"Failed to load the {} environment variable: {}",
						$env_var, e
					))
				},
			}
		};
	}

	macro_rules! read_config {
		($env:expr, $config: expr, $item: expr, $var_name: expr) => {
			$env.or($config).ok_or(format!(
				"{} must be provided in the configuration file or the environment variable {} must be set.",
				$item, $var_name
			))?
		};
	}

	let rsa_pem_env = read_env!(JWT_RSA_PEM_VAR);
	let rsa_pem = rsa_pem_env.or(jwt_auth_config.and_then(|config| config.rsa_pem));

	let username_env = read_env!(PSQL_USER_VAR);
	let password_env = read_env!(PSQL_PASS_VAR);
	let address_env: Option<SocketAddr> = read_env!(PSQL_ADDR_VAR)
		.map(|address| {
			address.parse().map_err(|e| {
				format!("Unable to parse the postgresql address environment variable: {}", e)
			})
		})
		.transpose()?;
	let default_db_env = read_env!(PSQL_DB_VAR);
	let vss_db_env = read_env!(PSQL_VSS_DB_VAR);
	let tls_config_env = read_env!(PSQL_TLS_VAR);
	let crt_pem_env = read_env!(PSQL_CERT_PEM_VAR);

	let (
		username_config,
		password_config,
		address_config,
		default_db_config,
		vss_db_config,
		tls_config,
	) = match postgresql_config {
		Some(c) => (
			c.username,
			c.password,
			c.address,
			c.default_database,
			c.vss_database,
			c.tls.map(|tls| tls.crt_pem),
		),
		None => (None, None, None, None, None, None),
	};

	let username =
		read_config!(username_env, username_config, "PostgreSQL database username", PSQL_USER_VAR);
	let password =
		read_config!(password_env, password_config, "PostgreSQL database password", PSQL_PASS_VAR);
	let address =
		read_config!(address_env, address_config, "PostgreSQL service address", PSQL_ADDR_VAR);
	let default_db = read_config!(
		default_db_env,
		default_db_config,
		"PostgreSQL default database name",
		PSQL_DB_VAR
	);
	let vss_db =
		read_config!(vss_db_env, vss_db_config, "PostgreSQL vss database name", PSQL_VSS_DB_VAR);

	let tls_config =
		crt_pem_env.map(|pem| Some(pem)).or(tls_config_env.map(|_| None)).or(tls_config);

	let postgresql_prefix = format!("postgresql://{}:{}@{}", username, password, address);

	Ok(Configuration { bind_address, rsa_pem, postgresql_prefix, default_db, vss_db, tls_config })
}
