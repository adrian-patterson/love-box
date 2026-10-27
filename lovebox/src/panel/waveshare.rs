//! Waveshare 7.5-inch V2 panel integration.
//!
//! The driver is intentionally available only on Linux ARM with the `hardware` feature.
//! The command sequences live in [`Lifecycle`], over a byte-level [`Device`], so native
//! tests check exactly what goes down the wire. They follow Waveshare's own drivers
//! (`EPD_7in5_V2.c`, `epd7in5_V2.py`); `epd-waveshare` was dropped 2026-10-27 because it
//! has no way to send the fast-refresh registers.

#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
use super::Refresh;
use super::{Panel, PanelError};
#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
use crate::frame::Frame;
#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
use std::time::Duration;

/// The SPI, GPIO and clock primitives the panel protocol is written in.
#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
trait Device {
    /// Pulses RST: high 20 ms, low 2 ms, high 20 ms.
    fn reset(&mut self) -> Result<(), String>;
    /// Sends one command byte (DC low).
    fn command(&mut self, command: u8) -> Result<(), String>;
    /// Sends data bytes (DC high).
    fn data(&mut self, data: &[u8]) -> Result<(), String>;
    /// True while BUSY is low (the V2 panel only updates it after a 0x71 status command).
    fn is_busy(&mut self) -> Result<bool, String>;
    fn delay_ms(&mut self, ms: u64);
}

/// Longest the panel may stay busy: a full refresh takes about 4 s.
#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
const BUSY_LIMIT: Duration = Duration::from_secs(20);

#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
struct Lifecycle<D> {
    device: D,
    refresh: Refresh,
}

#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
impl<D: Device> Lifecycle<D> {
    fn new(device: D) -> Self {
        Self {
            device,
            refresh: Refresh::Full,
        }
    }

    /// Wakes the panel (it is always in deep sleep between updates) and shows `frame` with
    /// one refresh. One refresh per update: its waveform already wipes the previous image,
    /// so the 2022 white pre-clear only doubled the flashing; dropped 2026-10-26.
    fn show(&mut self, frame: &Frame) -> Result<(), PanelError> {
        self.init()?;
        // Old-data RAM (0x10) gets the frame as is and new-data RAM (0x13) its inverse, as
        // Waveshare's drivers send them: the controller takes 1 = black in 0x13, the
        // inverse of Frame's 1 = white (the first on-panel test showed a negative,
        // 2026-10-26), and every pixel differs between the two, so every pixel is driven.
        let bytes = frame.as_packed_bytes();
        let wire: Vec<u8> = bytes.iter().map(|byte| !byte).collect();
        self.send(0x10, bytes)?;
        self.send(0x13, &wire)?;
        self.device.command(0x12).map_err(PanelError::Device)?;
        self.device.delay_ms(100);
        self.wait_until_idle()
    }

    fn init(&mut self) -> Result<(), PanelError> {
        self.device.reset().map_err(PanelError::Device)?;
        self.send(0x06, &[0x17, 0x17, 0x28, 0x17])?; // booster soft start
        self.send(0x01, &[0x07, 0x07, 0x28, 0x17])?; // power setting
        self.device.command(0x04).map_err(PanelError::Device)?; // power on
        self.device.delay_ms(100);
        self.wait_until_idle()?;
        self.send(0x00, &[0x1F])?; // panel setting: black/white, waveform from OTP
        self.send(0x61, &[0x03, 0x20, 0x01, 0xE0])?; // resolution 800 x 480
        self.send(0x15, &[0x00])?; // single SPI
        self.send(0x50, &[0x10, 0x07])?; // VCOM and data interval
        self.send(0x60, &[0x22])?; // TCON
        if self.refresh == Refresh::Fast {
            // Waveshare's EPD_7IN5_V2_Init_Fast: a stronger booster, then pin the
            // temperature the controller picks its OTP waveform by (0xE0 enables the
            // fixed value, 0xE5 sets it), which selects a shorter waveform with fewer
            // flashes (about 1.5 s instead of 4 s).
            self.send(0x06, &[0x27, 0x27, 0x18, 0x17])?;
            self.send(0xE0, &[0x02])?;
            self.send(0xE5, &[0x5A])?;
        }
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), PanelError> {
        self.send(0x50, &[0xF7])?; // border floating
        self.device.command(0x02).map_err(PanelError::Device)?; // power off
        self.wait_until_idle()?;
        self.send(0x07, &[0xA5]) // deep sleep
    }

    fn send(&mut self, command: u8, data: &[u8]) -> Result<(), PanelError> {
        self.device.command(command).map_err(PanelError::Device)?;
        self.device.data(data).map_err(PanelError::Device)
    }

    /// Polls BUSY after a 0x71 status command, as the V2 panel needs, up to [`BUSY_LIMIT`]
    /// of polling delays.
    fn wait_until_idle(&mut self) -> Result<(), PanelError> {
        let mut waited = Duration::ZERO;
        loop {
            self.device.command(0x71).map_err(PanelError::Device)?;
            if !self.device.is_busy().map_err(PanelError::Device)? {
                break;
            }
            if waited >= BUSY_LIMIT {
                return Err(PanelError::Timeout(BUSY_LIMIT));
            }
            self.device.delay_ms(5);
            waited += Duration::from_millis(5);
        }
        self.device.delay_ms(20);
        Ok(())
    }
}

#[cfg(all(feature = "hardware", target_os = "linux", target_arch = "arm"))]
mod linux_arm {
    use super::{Device, Lifecycle, Panel, PanelError, Refresh};
    use crate::frame::Frame;
    use rppal::gpio::{Gpio, InputPin, OutputPin};
    use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
    use std::thread;
    use std::time::Duration;

    /// spidev's default transfer limit (`bufsiz`); a frame is 48,000 bytes.
    const SPI_CHUNK: usize = 4096;

    struct HardwareDevice {
        spi: Spi,
        busy: InputPin,
        dc: OutputPin,
        rst: OutputPin,
    }

    impl HardwareDevice {
        fn new() -> Result<Self, PanelError> {
            let gpio = Gpio::new().map_err(device_error)?;
            let busy = gpio.get(24).map_err(device_error)?.into_input();
            let dc = gpio.get(25).map_err(device_error)?.into_output();
            let rst = gpio.get(17).map_err(device_error)?.into_output();
            let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 4_000_000, Mode::Mode0)
                .map_err(device_error)?;
            Ok(Self { spi, busy, dc, rst })
        }
    }

    fn device_error(error: impl std::fmt::Display) -> PanelError {
        PanelError::Device(error.to_string())
    }

    impl Device for HardwareDevice {
        fn reset(&mut self) -> Result<(), String> {
            self.rst.set_high();
            self.delay_ms(20);
            self.rst.set_low();
            self.delay_ms(2);
            self.rst.set_high();
            self.delay_ms(20);
            Ok(())
        }

        fn command(&mut self, command: u8) -> Result<(), String> {
            self.dc.set_low();
            self.spi.write(&[command]).map_err(|e| e.to_string())?;
            Ok(())
        }

        fn data(&mut self, data: &[u8]) -> Result<(), String> {
            self.dc.set_high();
            for chunk in data.chunks(SPI_CHUNK) {
                self.spi.write(chunk).map_err(|e| e.to_string())?;
            }
            Ok(())
        }

        fn is_busy(&mut self) -> Result<bool, String> {
            Ok(self.busy.is_low())
        }

        fn delay_ms(&mut self, ms: u64) {
            thread::sleep(Duration::from_millis(ms));
        }
    }

    pub(super) struct WavesharePanel {
        lifecycle: Lifecycle<HardwareDevice>,
    }

    impl Panel for WavesharePanel {
        fn show(&mut self, frame: &Frame) -> Result<(), PanelError> {
            self.lifecycle.show(frame)
        }

        fn clear(&mut self) -> Result<(), PanelError> {
            self.lifecycle.show(&Frame::white())
        }

        fn sleep(&mut self) -> Result<(), PanelError> {
            self.lifecycle.sleep()
        }

        fn set_refresh(&mut self, refresh: Refresh) {
            self.lifecycle.refresh = refresh;
        }

        fn name(&self) -> &'static str {
            "waveshare-7in5-v2"
        }
    }

    pub(super) fn create() -> Result<Box<dyn Panel>, PanelError> {
        Ok(Box::new(WavesharePanel {
            lifecycle: Lifecycle::new(HardwareDevice::new()?),
        }))
    }
}

/// Create the hardware panel on the display worker thread.
#[cfg(all(feature = "hardware", target_os = "linux", target_arch = "arm"))]
pub fn create_waveshare_panel() -> Result<Box<dyn Panel>, PanelError> {
    linux_arm::create()
}

/// Give a useful message when hardware support is not compiled for this target.
#[cfg(not(all(feature = "hardware", target_os = "linux", target_arch = "arm")))]
pub fn create_waveshare_panel() -> Result<Box<dyn Panel>, PanelError> {
    Err(PanelError::Device(
        "Waveshare panel support requires a Linux ARM build with --features hardware".to_owned(),
    ))
}

#[cfg(all(
    test,
    not(all(feature = "hardware", target_os = "linux", target_arch = "arm"))
))]
mod unsupported_tests {
    #[test]
    fn unsupported_target_returns_actionable_error_without_opening_devices() {
        let error = match super::create_waveshare_panel() {
            Ok(_) => panic!("unsupported target unexpectedly created a hardware panel"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("--features hardware"));
        assert!(error.to_string().contains("Linux ARM"));
    }
}

#[cfg(test)]
mod tests {
    use super::{Device, Lifecycle, Refresh};
    use crate::frame::Frame;
    use crate::panel::PanelError;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Reset,
        Command(u8),
        Data(Vec<u8>),
    }

    /// Records the wire traffic; BUSY reads busy `busy_polls` times, then idle, each wait.
    #[derive(Default)]
    struct FakeDevice {
        events: Vec<Event>,
        busy_polls: usize,
        polls_left: usize,
        stuck: bool,
    }

    impl Device for FakeDevice {
        fn reset(&mut self) -> Result<(), String> {
            self.events.push(Event::Reset);
            Ok(())
        }
        fn command(&mut self, command: u8) -> Result<(), String> {
            self.events.push(Event::Command(command));
            Ok(())
        }
        fn data(&mut self, data: &[u8]) -> Result<(), String> {
            self.events.push(Event::Data(data.to_vec()));
            Ok(())
        }
        fn is_busy(&mut self) -> Result<bool, String> {
            if self.stuck {
                return Ok(true);
            }
            if self.polls_left > 0 {
                self.polls_left -= 1;
                return Ok(true);
            }
            self.polls_left = self.busy_polls;
            Ok(false)
        }
        fn delay_ms(&mut self, _: u64) {}
    }

    /// Commands with their data, status polls (0x71) left out.
    fn sequence(events: &[Event]) -> Vec<(u8, Vec<u8>)> {
        let mut out: Vec<(u8, Vec<u8>)> = Vec::new();
        for event in events {
            match event {
                Event::Command(0x71) | Event::Reset => {}
                Event::Command(c) => out.push((*c, Vec::new())),
                Event::Data(d) => out.last_mut().unwrap().1.extend(d),
            }
        }
        out
    }

    fn init_commands(fast: bool) -> Vec<(u8, Vec<u8>)> {
        let mut init = vec![
            (0x06, vec![0x17, 0x17, 0x28, 0x17]),
            (0x01, vec![0x07, 0x07, 0x28, 0x17]),
            (0x04, vec![]),
            (0x00, vec![0x1F]),
            (0x61, vec![0x03, 0x20, 0x01, 0xE0]),
            (0x15, vec![0x00]),
            (0x50, vec![0x10, 0x07]),
            (0x60, vec![0x22]),
        ];
        if fast {
            init.extend([
                (0x06, vec![0x27, 0x27, 0x18, 0x17]),
                (0xE0, vec![0x02]),
                (0xE5, vec![0x5A]),
            ]);
        }
        init
    }

    #[test]
    fn full_refresh_resets_inits_sends_both_planes_refreshes_once_and_sleeps() {
        let mut panel = Lifecycle::new(FakeDevice::default());
        panel.show(&Frame::black()).unwrap();
        panel.sleep().unwrap();

        assert_eq!(panel.device.events[0], Event::Reset);
        assert_eq!(
            panel
                .device
                .events
                .iter()
                .filter(|e| **e == Event::Reset)
                .count(),
            1
        );
        let mut expected = init_commands(false);
        expected.extend([
            (0x10, vec![0x00; 48_000]),
            (0x13, vec![0xFF; 48_000]),
            (0x12, vec![]),
            (0x50, vec![0xF7]),
            (0x02, vec![]),
            (0x07, vec![0xA5]),
        ]);
        assert_eq!(sequence(&panel.device.events), expected);
    }

    #[test]
    fn fast_refresh_adds_waveshares_fast_init_registers() {
        let mut panel = Lifecycle::new(FakeDevice::default());
        panel.refresh = Refresh::Fast;
        panel.show(&Frame::white()).unwrap();
        let mut expected = init_commands(true);
        expected.extend([
            (0x10, vec![0xFF; 48_000]),
            (0x13, vec![0x00; 48_000]),
            (0x12, vec![]),
        ]);
        assert_eq!(sequence(&panel.device.events), expected);
    }

    #[test]
    fn new_data_plane_inverts_bits_and_preserves_pixel_order() {
        let mut panel = Lifecycle::new(FakeDevice::default());
        let mut patterned = Frame::white();
        patterned.set(0, 0, crate::frame::Pixel::Black);
        patterned.set(7, 0, crate::frame::Pixel::Black);
        patterned.set(8, 0, crate::frame::Pixel::Black);
        patterned.set(799, 479, crate::frame::Pixel::Black);
        panel.show(&patterned).unwrap();

        let sent = sequence(&panel.device.events);
        let (_, bytes) = sent.iter().find(|(c, _)| *c == 0x13).unwrap();
        // Frame bytes 0x7E, 0x7F, 0xFE (1 = white) go out inverted (1 = black).
        assert_eq!(bytes[0], 0x81);
        assert_eq!(bytes[1], 0x80);
        assert_eq!(bytes[47_999], 0x01);
        assert_eq!(bytes.len(), 48_000);
    }

    #[test]
    fn busy_is_polled_with_a_status_command_until_idle() {
        let mut panel = Lifecycle::new(FakeDevice {
            busy_polls: 3,
            polls_left: 3,
            ..FakeDevice::default()
        });
        panel.show(&Frame::white()).unwrap();
        // Two waits (after power on, after refresh), each 3 busy polls plus the idle one.
        let polls = panel
            .device
            .events
            .iter()
            .filter(|e| **e == Event::Command(0x71))
            .count();
        assert_eq!(polls, 8);
    }

    #[test]
    fn a_panel_that_never_goes_idle_times_out() {
        let mut panel = Lifecycle::new(FakeDevice {
            stuck: true,
            ..FakeDevice::default()
        });
        assert!(matches!(
            panel.show(&Frame::white()),
            Err(PanelError::Timeout(_))
        ));
    }
}
