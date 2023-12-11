use std::net::{IpAddr, ToSocketAddrs};

use clap::{Parser, ValueEnum};
use gamedig::{
    games::*,
    protocols::types::{CommonResponse, ExtraRequestSettings, TimeoutSettings},
};

mod error;

use self::error::{Error, Result};

// NOTE: For some reason without setting long_about here the doc comment for
// ExtraRequestSettings gets set as the about for the CLI.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Unique identifier of the game for which server information is being
    /// queried.
    #[arg(short, long)]
    game: String,

    /// Hostname or IP address of the server.
    #[arg(short, long)]
    ip: String,

    /// Optional query port number for the server. If not provided the default
    /// port for the game is used.
    #[arg(short, long)]
    port: Option<u16>,

    /// Flag indicating if the output should be in JSON format.
    #[cfg(feature = "json")]
    #[arg(short, long)]
    json: bool,

    /// Which response variant to use when outputting.
    #[arg(short, long, default_value = "generic")]
    output_mode: OutputMode,

    /// Optional timeout settings for the server query.
    #[command(flatten, next_help_heading = "Timeouts")]
    timeout_settings: Option<TimeoutSettings>,

    /// Optional extra settings for the server query.
    #[command(flatten, next_help_heading = "Query options")]
    extra_options: Option<ExtraRequestSettings>,
}

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputMode {
    /// A generalised response that maps common fields from all game types to
    /// the same name.
    Generic,
    /// The raw result returned from the protocol query, formatted similarly to
    /// how the server returned it.
    ProtocolSpecific,
}

/// Attempt to find a game from the [library game definitions](GAMES) based on
/// its unique identifier.
///
/// # Arguments
/// * `game_id` - A string slice containing the unique game identifier.
///
/// # Returns
/// * Result<&'static [Game]> - On sucess returns a reference to the game
///   definition; on failure returns a [Error::UnknownGame] error.
fn find_game(game_id: &str) -> Result<&'static Game> {
    // Attempt to retrieve the game from the predefined game list
    GAMES
        .get(game_id)
        .ok_or_else(|| Error::UnknownGame(game_id.to_string()))
}

/// Resolve an IP address by either parsing an IP address or doing a DNS lookup.
/// In the case of DNS lookup update extra request options with the hostname.
///
/// # Arguments
/// * `host` - A string slice containing the IP address or hostname of a server
///   to resolve.
/// * `extra_options` - Mutable reference to extra options for the game query.
///
/// # Returns
/// * `Result<IpAddr>` - On sucess returns a resolved IP address; on failure
///   returns an [Error::InvalidHostname] error.
fn resolve_ip_or_domain(host: &str, extra_options: &mut Option<ExtraRequestSettings>) -> Result<IpAddr> {
    if let Ok(parsed_ip) = host.parse() {
        Ok(parsed_ip)
    } else {
        set_hostname_if_missing(host, extra_options);

        resolve_domain(host)
    }
}

/// Resolve a domain name to one of its IP addresses (the first one returned).
///
/// # Arguments
/// * `domain` - A string slice containing the domain name to lookup.
///
/// # Returns
/// * `Result<IpAddr>` - On success, returns one of the resolved IP addresses;
///   on failure returns an [Error::InvalidHostname] error.
fn resolve_domain(domain: &str) -> Result<IpAddr> {
    // Append a dummy port to perform socket address resolution and then extract the
    // IP
    Ok(format!("{}:0", domain)
        .to_socket_addrs()
        .map_err(|_| Error::InvalidHostname(domain.to_string()))?
        .next()
        .ok_or_else(|| Error::InvalidHostname(domain.to_string()))?
        .ip())
}

/// Sets the hostname on extra request settings if it is not already set.
///
/// # Arguments
/// * `host` - A string slice containing the hostname.
/// * `extra_options` - A mutable reference to optional [ExtraRequestSettings].
fn set_hostname_if_missing(host: &str, extra_options: &mut Option<ExtraRequestSettings>) {
    if let Some(extra_options) = extra_options {
        if extra_options.hostname.is_none() {
            // If extra_options exists but hostname is None overwrite hostname in place
            extra_options.hostname = Some(host.to_string())
        }
    } else {
        // If extra_options is None create default settings with hostname
        *extra_options = Some(ExtraRequestSettings::default().set_hostname(host.to_string()));
    }
}

/// Output the result of a query to stdout.
///
/// # Arguments
/// * `args` - A reference to the command line options.
/// * `result` - A reference to the result of the query.
fn output_result(args: &Cli, result: &dyn CommonResponse) {
    match args.output_mode {
        #[cfg(feature = "json")]
        OutputMode::Generic if args.json => output_result_json(result.as_json()),
        #[cfg(feature = "json")]
        OutputMode::ProtocolSpecific if args.json => output_result_json(result.as_original()),

        OutputMode::Generic => output_result_debug(result.as_json()),
        OutputMode::ProtocolSpecific => output_result_debug(result.as_original()),
    }
}

/// Output the result using debug formatting.
///
/// # Arguments
/// * `result` - A result that can be output using the debug formatter.
fn output_result_debug<R: std::fmt::Debug>(result: R) {
    println!("{:#?}", result);
}

/// Output the result as a JSON object.
///
/// # Arguments
/// * `result` - A serde serializable result.
#[cfg(feature = "json")]
fn output_result_json<R: serde::Serialize>(result: R) {
    serde_json::to_writer_pretty(std::io::stdout(), &result).unwrap();
}

fn main() -> Result<()> {
    // Parse the command line arguments
    let args = Cli::parse();

    // Retrieve the game based on the provided ID
    let game = find_game(&args.game)?;

    // Extract extra options for use in setup
    let mut extra_options = args.extra_options.clone();

    // Resolve the IP address
    let ip = resolve_ip_or_domain(&args.ip, &mut extra_options)?;

    // Query the server using game definition, parsed IP, and user command line
    // flags.
    let result = query_with_timeout_and_extra_settings(game, &ip, args.port, args.timeout_settings, extra_options)?;

    // Output the result in the specified format
    output_result(&args, result.as_ref());

    Ok(())
}