//! Waveshare 2.13" Touch e-Paper HAT (V4): SSD1680 e-paper over SPI + GT1151 touch over I2C.
//! Ported from Waveshare's Python driver (github.com/waveshareteam/Touch_e-Paper_HAT,
//! python/lib/TP_lib/epd2in13_V4.py and gt1151.py).

use std::thread;
use std::time::{Duration, Instant};

use rppal::gpio::{Gpio, InputPin, OutputPin};
use rppal::i2c::I2c;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};

use crate::Result;

// BCM pin numbers, from Waveshare's epdconfig.py
const EPD_RST: u8 = 17;
const EPD_DC: u8 = 25;
const EPD_BUSY: u8 = 24;
const TP_RST: u8 = 22;
const TP_INT: u8 = 27;
const TP_ADDR: u16 = 0x14;

const WIDTH: u16 = 122; // panel's native (portrait) size
const HEIGHT: u16 = 250;

fn ms(n: u64) {
    thread::sleep(Duration::from_millis(n));
}

pub struct Epd {
    spi: Spi,
    rst: OutputPin,
    dc: OutputPin,
    busy: InputPin,
}

impl Epd {
    pub fn new(gpio: &Gpio) -> Result<Self> {
        Ok(Self {
            spi: Spi::new(Bus::Spi0, SlaveSelect::Ss0, 10_000_000, Mode::Mode0)?,
            rst: gpio.get(EPD_RST)?.into_output(),
            dc: gpio.get(EPD_DC)?.into_output(),
            busy: gpio.get(EPD_BUSY)?.into_input_pulldown(),
        })
    }

    fn cmd(&mut self, c: u8) -> Result<()> {
        self.dc.set_low();
        self.spi.write(&[c])?;
        Ok(())
    }

    fn data(&mut self, d: &[u8]) -> Result<()> {
        self.dc.set_high();
        for chunk in d.chunks(4096) {
            // spidev's default transfer limit
            self.spi.write(chunk)?;
        }
        Ok(())
    }

    fn cmd_data(&mut self, c: u8, d: &[u8]) -> Result<()> {
        self.cmd(c)?;
        self.data(d)
    }

    /// Waits until the panel is idle. Returns whether it reported busy at all, giving
    /// it 20 ms to raise the line first (0: idle, 1: busy).
    fn wait_busy(&mut self) -> Result<bool> {
        let start = Instant::now();
        let mut was_busy = false;
        loop {
            if self.busy.is_high() {
                was_busy = true;
                if start.elapsed() > Duration::from_secs(10) {
                    return Err("e-paper stayed busy for 10 s".into());
                }
                ms(10);
            } else if was_busy || start.elapsed() > Duration::from_millis(20) {
                return Ok(was_busy);
            } else {
                ms(1);
            }
        }
    }

    fn window_and_cursor(&mut self) -> Result<()> {
        let (x_end, y_end) = ((WIDTH - 1) >> 3, HEIGHT - 1);
        self.cmd_data(0x44, &[0, x_end as u8])?; // RAM X start/end (in bytes)
        self.cmd_data(0x45, &[0, 0, y_end as u8, (y_end >> 8) as u8])?; // RAM Y start/end
        self.cmd_data(0x4E, &[0])?; // RAM X counter
        self.cmd_data(0x4F, &[0, 0]) // RAM Y counter
    }

    /// Full-refresh mode. Flashes the panel; clears ghosting.
    pub fn init_full(&mut self) -> Result<()> {
        for (level, wait) in [(true, 20), (false, 2), (true, 20)] {
            self.rst.write(level.into());
            ms(wait);
        }
        self.wait_busy()?;
        self.cmd(0x12)?; // SWRESET
        self.wait_busy()?;
        self.cmd_data(0x01, &[0xF9, 0x00, 0x00])?; // driver output control
        self.cmd_data(0x11, &[0x03])?; // data entry mode
        self.window_and_cursor()?;
        self.cmd_data(0x3C, &[0x05])?; // border waveform
        self.cmd_data(0x21, &[0x00, 0x80])?; // display update control
        self.cmd_data(0x18, &[0x80])?; // internal temperature sensor
        self.wait_busy()?;
        Ok(())
    }

    /// Partial-refresh mode: same as init_full's register setup, without the software reset.
    fn init_part(&mut self) -> Result<()> {
        self.rst.set_low();
        ms(1);
        self.rst.set_high();
        self.cmd_data(0x01, &[0xF9, 0x00, 0x00])?;
        self.cmd_data(0x3C, &[0x80])?;
        self.cmd_data(0x11, &[0x03])?;
        self.window_and_cursor()
    }

    fn update(&mut self, mode: u8) -> Result<()> {
        self.cmd_data(0x22, &[mode])?; // display update sequence
        self.cmd(0x20)?; // activate
        // A refresh always holds BUSY high for a while, so a panel that never raises it
        // isn't receiving commands (seen with the HAT not seated on all 40 pins)
        if !self.wait_busy()? {
            return Err("screen not responding (busy line never went high); check the HAT is seated on all 40 pins".into());
        }
        Ok(())
    }

    /// Full refresh that also stores the image as the base for later partial refreshes.
    pub fn display_base(&mut self, buf: &[u8]) -> Result<()> {
        self.init_full()?;
        self.cmd_data(0x24, buf)?; // new image RAM
        self.cmd_data(0x26, buf)?; // previous image RAM
        self.update(0xF7)
    }

    pub fn display_partial(&mut self, buf: &[u8]) -> Result<()> {
        self.init_part()?;
        self.cmd_data(0x24, buf)?;
        self.update(0xFF)
    }

    pub fn clear(&mut self) -> Result<()> {
        self.init_full()?;
        self.cmd_data(0x24, &vec![0xFF; (WIDTH as usize).div_ceil(8) * HEIGHT as usize])?;
        self.update(0xF7)
    }

    pub fn sleep(&mut self) -> Result<()> {
        self.cmd_data(0x10, &[0x01]) // deep sleep
    }
}

/// Starts a thread that calls `on_tap` on every tap of the touch panel, until it returns false.
pub fn spawn_touch(gpio: &Gpio, on_tap: impl Fn() -> bool + Send + 'static) -> Result<()> {
    let mut i2c = I2c::with_bus(1)?;
    i2c.set_slave_address(TP_ADDR)?;
    let mut rst = gpio.get(TP_RST)?.into_output();
    let int = gpio.get(TP_INT)?.into_input_pulldown();
    for level in [true, false, true] {
        rst.write(level.into());
        ms(100);
    }

    let read = |i2c: &mut I2c, reg: u16, buf: &mut [u8]| -> rppal::i2c::Result<()> {
        i2c.write(&[(reg >> 8) as u8, reg as u8])?;
        i2c.read(buf).map(|_| ())
    };
    let write = |i2c: &mut I2c, reg: u16, v: u8| i2c.write(&[(reg >> 8) as u8, reg as u8, v]).map(|_| ());

    thread::spawn(move || {
        // A touch is a run of reports; it has ended once none has come for this long
        const RELEASED_AFTER: Duration = Duration::from_millis(300);
        // Failed reads in a row (about 5 s of them) before the controller is reset
        const RESET_AFTER: u32 = 250;
        let mut last_report: Option<Instant> = None;
        let mut failures = 0u32;
        loop {
            ms(20);
            if int.is_high() {
                continue; // INT is pulled low while the panel reports a touch
            }
            let mut status = [0u8];
            if let Err(e) = read(&mut i2c, 0x814E, &mut status) {
                if failures == 0 {
                    eprintln!("touch: {e}");
                }
                failures += 1;
                if failures % RESET_AFTER == 0 {
                    for level in [false, true] {
                        rst.write(level.into());
                        ms(100);
                    }
                }
                continue;
            }
            if failures > 0 {
                eprintln!("touch: responding again");
                failures = 0;
            }
            let _ = write(&mut i2c, 0x814E, 0); // acknowledge
            let points = status[0] & 0x0F;
            if status[0] & 0x80 != 0 && (1..=5).contains(&points) {
                // Only the first report of a touch is a tap, so a finger held on the screen
                // (or something resting on it) toggles once instead of over and over
                let new_touch = last_report.map_or(true, |t| t.elapsed() > RELEASED_AFTER);
                last_report = Some(Instant::now());
                if new_touch && !on_tap() {
                    return;
                }
            }
        }
    });
    Ok(())
}
