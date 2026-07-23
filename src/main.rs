use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod app;
mod config;
mod cpu;
mod gauge;
mod history;
mod theme;
mod ui;

use app::App;

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

const TICK: Duration = Duration::from_secs(1);
const BACKEND_NO_DROP_ENV: &str = "AGT_NO_DROP";

struct SampleClock {
    last_sample: Instant,
}

impl SampleClock {
    fn new(now: Instant) -> Self {
        Self { last_sample: now }
    }

    fn poll_timeout(&self, now: Instant) -> Duration {
        TICK.saturating_sub(now.saturating_duration_since(self.last_sample))
    }

    fn take_due_sample(&mut self, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.last_sample);
        if elapsed < TICK {
            return None;
        }

        self.last_sample = now;
        Some(elapsed)
    }
}

fn main() -> io::Result<()> {
    if handle_cli()? {
        return Ok(());
    }

    configure_backend();
    let mut app = App::init();
    let mut session = TerminalSession::start()?;
    let run_result = run(session.terminal_mut(), &mut app);
    let restore_result = session.restore();

    match run_result {
        Err(error) => Err(error),
        Ok(()) => restore_result,
    }
}

fn configure_backend() {
    if should_set_no_drop(std::env::var_os(BACKEND_NO_DROP_ENV).as_deref()) {
        // SAFETY: This runs on the initial thread before libamdgpu_top creates
        // its worker threads. No other thread can concurrently access the
        // process environment at this point.
        unsafe { std::env::set_var(BACKEND_NO_DROP_ENV, "1") };
    }
}

fn should_set_no_drop(current: Option<&std::ffi::OsStr>) -> bool {
    current.is_none()
}

fn handle_cli() -> io::Result<bool> {
    let mut args = std::env::args_os().skip(1);
    let Some(arg) = args.next() else {
        return Ok(false);
    };

    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "amdtop accepts at most one option",
        ));
    }

    match arg.to_str() {
        Some("-h" | "--help") => {
            println!(
                "amdtop {}\n\nUsage: amdtop [OPTIONS]\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version",
                env!("CARGO_PKG_VERSION")
            );
            Ok(true)
        }
        Some("-V" | "--version") => {
            println!("amdtop {}", env!("CARGO_PKG_VERSION"));
            Ok(true)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown option: {}", arg.to_string_lossy()),
        )),
    }
}

struct TerminalSession {
    terminal: Tui,
    active: bool,
}

impl TerminalSession {
    fn start() -> io::Result<Self> {
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }

        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error);
            }
        };

        Ok(Self {
            terminal,
            active: true,
        })
    }

    fn terminal_mut(&mut self) -> &mut Tui {
        &mut self.terminal
    }

    fn restore(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }

        let raw_result = disable_raw_mode();
        let screen_result = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let cursor_result = self.terminal.show_cursor();
        self.active = false;

        raw_result.and(screen_result).and(cursor_result)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn is_device_refresh_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('r') && key.modifiers == KeyModifiers::NONE
}

fn run(terminal: &mut Tui, app: &mut App) -> io::Result<()> {
    let mut sample_clock = SampleClock::new(Instant::now());
    app.sample(TICK);
    terminal.draw(|frame| ui::draw(frame, app))?;

    loop {
        let timeout = sample_clock.poll_timeout(Instant::now());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if is_quit_key(&key) {
                app.save_state()?;
                return Ok(());
            }

            if is_device_refresh_key(&key) {
                app.refresh_devices();
            } else {
                match key.code {
                    KeyCode::Tab => app.next_section(),
                    KeyCode::BackTab => app.prev_section(),
                    KeyCode::Char(' ') | KeyCode::Enter => app.toggle_collapse()?,
                    KeyCode::Char('t') => app.cycle_theme(true)?,
                    KeyCode::Char('T') => app.cycle_theme(false)?,
                    KeyCode::Char('b') => app.cycle_block(true)?,
                    KeyCode::Char('B') => app.cycle_block(false)?,
                    _ => {}
                }
            }
        }

        if let Some(elapsed) = sample_clock.take_due_sample(Instant::now()) {
            app.sample(elapsed);
        }
        terminal.draw(|frame| ui::draw(frame, app))?;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{SampleClock, TICK, is_device_refresh_key, is_quit_key, should_set_no_drop};

    #[test]
    fn backend_keeps_device_handles_unless_the_user_overrides_it() {
        assert!(should_set_no_drop(None));
        assert!(!should_set_no_drop(Some("0".as_ref())));
        assert!(!should_set_no_drop(Some("1".as_ref())));
    }

    #[test]
    fn ctrl_c_is_a_quit_key() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        assert!(is_quit_key(&key));
        assert!(!is_quit_key(&KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
    }

    #[test]
    fn plain_r_is_the_hidden_device_refresh_key() {
        assert!(is_device_refresh_key(&KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::NONE
        )));
        assert!(!is_device_refresh_key(&KeyEvent::new(
            KeyCode::Char('R'),
            KeyModifiers::SHIFT
        )));
        assert!(!is_device_refresh_key(&KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn sample_clock_waits_for_the_tick_interval() {
        let start = Instant::now();
        let clock = SampleClock::new(start);

        assert_eq!(clock.poll_timeout(start), TICK);
        assert_eq!(
            clock.poll_timeout(start + Duration::from_millis(250)),
            Duration::from_millis(750)
        );
        assert_eq!(clock.poll_timeout(start + TICK), Duration::ZERO);
    }

    #[test]
    fn sample_clock_reports_real_elapsed_time_and_resets() {
        let start = Instant::now();
        let mut clock = SampleClock::new(start);

        assert_eq!(
            clock.take_due_sample(start + Duration::from_millis(999)),
            None
        );
        assert_eq!(
            clock.take_due_sample(start + Duration::from_millis(1_250)),
            Some(Duration::from_millis(1_250))
        );
        assert_eq!(clock.take_due_sample(start + Duration::from_secs(2)), None);
    }
}
