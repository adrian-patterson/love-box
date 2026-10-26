//! Waveshare 7.5-inch V2 panel integration.
//!
//! The driver is intentionally available only on Linux ARM with the `hardware` feature.
//! Lifecycle sequencing is kept separate from GPIO/SPI so native tests can exercise it.

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
trait Device {
    fn wake_up(&mut self) -> Result<(), String>;
    fn update_and_refresh(&mut self, frame: &[u8]) -> Result<(), String>;
    fn wait_until_idle(&mut self) -> Result<(), String>;
    fn sleep(&mut self) -> Result<(), String>;
}

#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
struct Lifecycle<D> {
    device: D,
}

#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
impl<D: Device> Lifecycle<D> {
    fn show(&mut self, frame: &Frame) -> Result<(), PanelError> {
        self.device.wake_up().map_err(device_error)?;
        // One full refresh per update. Its waveform already wipes the previous image, so
        // the 2022 white pre-clear only doubled the flashing (~6 flashes, ~8 s) on the
        // first on-panel test; dropped 2026-10-26.
        self.refresh(frame)
    }

    fn refresh(&mut self, frame: &Frame) -> Result<(), PanelError> {
        // The V2 controller's new-data register (0x13) takes 1 = black, the inverse of
        // Frame's 1 = white; Waveshare's own drivers send `~image` (EPD_7IN5_V2_Display in C,
        // `buf[i] ^= 0xFF` in Python). epd-waveshare passes bytes through unchanged, which
        // showed a negative on the first on-panel test (2026-10-26).
        let wire: Vec<u8> = frame.as_packed_bytes().iter().map(|byte| !byte).collect();
        self.device
            .update_and_refresh(&wire)
            .map_err(device_error)?;
        // Upstream display_frame sends DisplayRefresh and returns immediately. Wait here so
        // callers never report success or enter deep sleep before the full refresh is done.
        self.device.wait_until_idle().map_err(device_error)
    }

    fn sleep(&mut self) -> Result<(), PanelError> {
        self.device.sleep().map_err(device_error)
    }
}

#[cfg(any(
    test,
    all(feature = "hardware", target_os = "linux", target_arch = "arm")
))]
fn device_error(message: String) -> PanelError {
    PanelError::Device(message)
}

#[cfg(all(feature = "hardware", target_os = "linux", target_arch = "arm"))]
mod linux_arm {
    use super::{Device, Lifecycle, Panel, PanelError};
    use crate::frame::Frame;
    use epd_waveshare::epd7in5_v2::{Epd7in5, HEIGHT, WIDTH};
    use epd_waveshare::prelude::WaveshareDisplay;
    use rppal::gpio::Gpio;
    use rppal::hal::Delay;
    use rppal::spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi};

    struct HardwareDevice {
        spi: SimpleHalSpiDevice,
        epd: Epd7in5<
            SimpleHalSpiDevice,
            rppal::gpio::InputPin,
            rppal::gpio::OutputPin,
            rppal::gpio::OutputPin,
            Delay,
        >,
        delay: Delay,
    }

    impl HardwareDevice {
        fn new() -> Result<Self, PanelError> {
            let gpio = Gpio::new().map_err(|error| PanelError::Device(error.to_string()))?;
            let busy = gpio
                .get(24)
                .map_err(|error| PanelError::Device(error.to_string()))?
                .into_input();
            let dc = gpio
                .get(25)
                .map_err(|error| PanelError::Device(error.to_string()))?
                .into_output();
            let rst = gpio
                .get(17)
                .map_err(|error| PanelError::Device(error.to_string()))?
                .into_output();
            let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 4_000_000, Mode::Mode0)
                .map_err(|error| PanelError::Device(error.to_string()))?;
            let mut spi = SimpleHalSpiDevice::new(spi);
            let mut delay = Delay::new();
            let epd = Epd7in5::new(&mut spi, busy, dc, rst, &mut delay, None)
                .map_err(|error| PanelError::Device(error.to_string()))?;
            Ok(Self { spi, epd, delay })
        }
    }

    impl Device for HardwareDevice {
        fn wake_up(&mut self) -> Result<(), String> {
            self.epd
                .wake_up(&mut self.spi, &mut self.delay)
                .map_err(|error| error.to_string())
        }

        fn update_and_refresh(&mut self, frame: &[u8]) -> Result<(), String> {
            if frame.len() != (WIDTH as usize / 8) * HEIGHT as usize {
                return Err(format!("expected 48000 frame bytes, got {}", frame.len()));
            }
            self.epd
                .update_and_display_frame(&mut self.spi, frame, &mut self.delay)
                .map_err(|error| error.to_string())
        }

        fn wait_until_idle(&mut self) -> Result<(), String> {
            self.epd
                .wait_until_idle(&mut self.spi, &mut self.delay)
                .map_err(|error| error.to_string())
        }

        fn sleep(&mut self) -> Result<(), String> {
            self.epd
                .sleep(&mut self.spi, &mut self.delay)
                .map_err(|error| error.to_string())
        }
    }

    pub(super) struct WavesharePanel {
        lifecycle: Lifecycle<HardwareDevice>,
    }

    impl WavesharePanel {
        fn new() -> Result<Self, PanelError> {
            Ok(Self {
                lifecycle: Lifecycle {
                    device: HardwareDevice::new()?,
                },
            })
        }
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

        fn name(&self) -> &'static str {
            "waveshare-7in5-v2"
        }
    }

    pub(super) fn create() -> Result<Box<dyn Panel>, PanelError> {
        Ok(Box::new(WavesharePanel::new()?))
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
    use super::{Device, Lifecycle};
    use crate::frame::Frame;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Wake,
        Refresh(Vec<u8>),
        Wait,
        Sleep,
    }

    struct FakeDevice(Arc<Mutex<Vec<Event>>>);

    impl Device for FakeDevice {
        fn wake_up(&mut self) -> Result<(), String> {
            self.0.lock().unwrap().push(Event::Wake);
            Ok(())
        }

        fn update_and_refresh(&mut self, frame: &[u8]) -> Result<(), String> {
            self.0.lock().unwrap().push(Event::Refresh(frame.to_vec()));
            Ok(())
        }

        fn wait_until_idle(&mut self) -> Result<(), String> {
            self.0.lock().unwrap().push(Event::Wait);
            Ok(())
        }

        fn sleep(&mut self) -> Result<(), String> {
            self.0.lock().unwrap().push(Event::Sleep);
            Ok(())
        }
    }

    #[test]
    fn wake_shows_frame_with_one_refresh_waits_and_sleeps() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut panel = Lifecycle {
            device: FakeDevice(events.clone()),
        };
        let frame = Frame::black();
        panel.show(&frame).unwrap();
        panel.sleep().unwrap();

        let actual = events.lock().unwrap();
        assert_eq!(
            actual.as_slice(),
            [
                Event::Wake,
                Event::Refresh(vec![0xFF; 48_000]),
                Event::Wait,
                Event::Sleep
            ]
        );
    }

    #[test]
    fn frame_mapping_inverts_bits_for_the_v2_wire_and_preserves_pixel_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut panel = Lifecycle {
            device: FakeDevice(events.clone()),
        };
        let mut patterned = Frame::white();
        patterned.set(0, 0, crate::frame::Pixel::Black);
        patterned.set(7, 0, crate::frame::Pixel::Black);
        patterned.set(8, 0, crate::frame::Pixel::Black);
        patterned.set(799, 479, crate::frame::Pixel::Black);
        panel.show(&patterned).unwrap();

        let actual = events.lock().unwrap();
        assert_eq!(actual.len(), 3);
        let Event::Refresh(bytes) = &actual[1] else {
            panic!("expected show refresh")
        };
        // Frame bytes 0x7E, 0x7F, 0xFE (1 = white) go out inverted (1 = black).
        assert_eq!(bytes[0], 0x81);
        assert_eq!(bytes[1], 0x80);
        assert_eq!(bytes[47_999], 0x01);
        assert_eq!(bytes.len(), 48_000);
    }

    #[test]
    fn white_clear_refreshes_once_after_wake() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut panel = Lifecycle {
            device: FakeDevice(events.clone()),
        };
        panel.show(&Frame::white()).unwrap();
        let actual = events.lock().unwrap();
        assert_eq!(
            actual.as_slice(),
            [Event::Wake, Event::Refresh(vec![0x00; 48_000]), Event::Wait]
        );
    }

    struct FailingDevice;

    impl Device for FailingDevice {
        fn wake_up(&mut self) -> Result<(), String> {
            Err("fake wake error".to_owned())
        }
        fn update_and_refresh(&mut self, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
        fn wait_until_idle(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn sleep(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn device_errors_propagate_to_panel_error() {
        let mut panel = Lifecycle {
            device: FailingDevice,
        };
        assert!(matches!(
            panel.show(&Frame::black()),
            Err(crate::panel::PanelError::Device(message)) if message == "fake wake error"
        ));
    }
}
