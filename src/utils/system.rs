use std::io;
use std::io::Write;
use std::process;
use std::env;
use std::borrow::Cow;

use regex::{Regex, Captures};
use chrono::{DateTime, UTC};

#[cfg(not(windows))]
use chan_signal::{notify, Signal};

use prelude::*;

#[cfg(not(windows))]
pub fn run_or_interrupt<F>(f: F)
    where F: FnOnce() -> (), F: Send + 'static
{
    use chan;
    let run = |_sdone: chan::Sender<()>| f();
    let signal = notify(&[Signal::INT, Signal::TERM]);
    let (sdone, rdone) = chan::sync(0);
    ::std::thread::spawn(move || run(sdone));

    let mut rv = None;

    chan_select! {
        signal.recv() -> signal => { rv = signal; },
        rdone.recv() => {}
    }

    if let Some(signal) = rv {
        use chan_signal::Signal;
        if signal == Signal::INT {
            println!("Interrupted!");
        }
    }
}

#[cfg(windows)]
pub fn run_or_interrupt<F>(f: F)
    where F: FnOnce() -> (), F: Send + 'static
{
    f();
}

/// Propagate an exit status outwarts
pub fn propagate_exit_status(status: process::ExitStatus) {
    if !status.success() {
        if let Some(code) = status.code() {
            process::exit(code);
        } else {
            process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn is_homebrew_install_result() -> Result<bool> {
    let mut exe = env::current_exe()?.canonicalize()?;
    exe.pop();
    exe.set_file_name("INSTALL_RECEIPT.json");
    Ok(exe.is_file())
}

#[cfg(windows)]
fn is_homebrew_install_result() -> Result<bool> {
    Ok(false)
}

fn is_npm_install_result() -> Result<bool> {
    let mut exe = env::current_exe()?.canonicalize()?;
    exe.pop();
    exe.set_file_name("package.json");
    Ok(exe.is_file())
}

/// Checks if we were installed from homebrew
pub fn is_homebrew_install() -> bool {
    is_homebrew_install_result().unwrap_or(false)
}

/// Checks if we were installed via npm
pub fn is_npm_install() -> bool {
    is_npm_install_result().unwrap_or(false)
}

/// Expands environment variables in a string
pub fn expand_envvars<'a>(s: &'a str) -> Cow<'a, str> {
    expand_vars(s, |key| {
        env::var(key).unwrap_or("".into())
    })
}

/// Expands variables in a string
pub fn expand_vars<'a, F: Fn(&str) -> String>(s: &'a str, f: F) -> Cow<'a, str> {
    lazy_static! {
        static ref VAR_RE: Regex = Regex::new(
            r"\$(\$|[a-zA-Z0-9_]+|\([^)]+\)|\{[^}]+\})").unwrap();
    }
    VAR_RE.replace_all(s, |caps: &Captures| {
        let key = &caps[1];
        if key == "$" {
            "$".into()
        } else if &key[..1] == "(" || &key[..1] == "{" {
            f(&key[1..key.len() - 1])
        } else {
            f(key)
        }
    })
}

/// Helper that renders an error to stderr.
pub fn print_error(err: &Error) {
    use std::error::Error;

    if let &ErrorKind::Clap(ref clap_err) = err.kind() {
        clap_err.exit();
    }

    writeln!(&mut io::stderr(), "error: {}", err).ok();
    let mut cause = err.cause();
    while let Some(the_cause) = cause {
        writeln!(&mut io::stderr(), "  caused by: {}", the_cause).ok();
        cause = the_cause.cause();
    }

    if env::var("RUST_BACKTRACE") == Ok("1".into()) {
        writeln!(&mut io::stderr(), "").ok();
        writeln!(&mut io::stderr(), "{:?}", err.backtrace()).ok();
    }
}

/// Given a system time returns the unix timestamp as f64
pub fn to_timestamp(tm: DateTime<UTC>) -> f64 {
    tm.timestamp() as f64
}

/// Initializes the backtrace support
pub fn init_backtrace() {
    use backtrace::Backtrace;
    use std::panic;
    use std::thread;

    panic::set_hook(Box::new(|info| {
        let backtrace = Backtrace::new();

        let thread = thread::current();
        let thread = thread.name().unwrap_or("unnamed");

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => {
                match info.payload().downcast_ref::<String>() {
                    Some(s) => &**s,
                    None => "Box<Any>",
                }
            }
        };

        match info.location() {
            Some(location) => {
                println_stderr!("thread '{}' panicked at '{}': {}:{}\n\n{:?}",
                         thread,
                         msg,
                         location.file(),
                         location.line(),
                         backtrace);
            }
            None => println_stderr!("thread '{}' panicked at '{}'{:?}", thread, msg, backtrace),
        }
    }));
}
