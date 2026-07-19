use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
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

    let mut app = App::init();
    let mut session = TerminalSession::start()?;
    let run_result = run(session.terminal_mut(), &mut app);
    let restore_result = session.restore();

    match run_result {
        Err(error) => Err(error),
        Ok(()) => restore_result,
    }
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
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.save_state()?;
                    return Ok(());
                }
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

        if let Some(elapsed) = sample_clock.take_due_sample(Instant::now()) {
            app.sample(elapsed);
        }
        terminal.draw(|frame| ui::draw(frame, app))?;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{SampleClock, TICK};

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
