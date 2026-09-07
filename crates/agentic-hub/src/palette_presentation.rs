use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteMainOrigin {
    HiddenApp,
    FocusedMain,
    ExternalApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteDismissal {
    RestoreHiddenApp,
    RefocusMain,
    None,
}

pub(crate) fn main_origin_before_palette(
    app_hidden: bool,
    main_focused: bool,
) -> PaletteMainOrigin {
    if app_hidden {
        PaletteMainOrigin::HiddenApp
    } else if main_focused {
        PaletteMainOrigin::FocusedMain
    } else {
        PaletteMainOrigin::ExternalApp
    }
}

pub(crate) fn dismissal_after_palette(origin: PaletteMainOrigin) -> PaletteDismissal {
    match origin {
        PaletteMainOrigin::HiddenApp => PaletteDismissal::RestoreHiddenApp,
        PaletteMainOrigin::FocusedMain => PaletteDismissal::RefocusMain,
        PaletteMainOrigin::ExternalApp => PaletteDismissal::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PalettePresentation {
    Pending,
    Active(PaletteMainOrigin),
}

#[derive(Debug, Default)]
struct PalettePresentationData {
    revision: u64,
    presentation: Option<PalettePresentation>,
}

#[derive(Debug, Default)]
pub(crate) struct PalettePresentationState {
    data: Mutex<PalettePresentationData>,
}

impl PalettePresentationState {
    fn advance(data: &mut PalettePresentationData) -> u64 {
        data.revision = data.revision.wrapping_add(1);
        data.revision
    }

    pub(crate) fn request_show(&self) -> Option<u64> {
        self.data.lock().ok().map(|mut data| {
            let request = Self::advance(&mut data);
            if data.presentation.is_none() {
                data.presentation = Some(PalettePresentation::Pending);
            }
            request
        })
    }

    pub(crate) fn request_hide(&self) -> Option<u64> {
        self.data
            .lock()
            .ok()
            .map(|mut data| Self::advance(&mut data))
    }

    pub(crate) fn start(&self, request: u64, origin: PaletteMainOrigin) -> bool {
        self.data
            .lock()
            .map(|mut data| {
                if data.revision != request || data.presentation.is_none() {
                    return false;
                }
                data.presentation = Some(PalettePresentation::Active(origin));
                true
            })
            .unwrap_or(false)
    }

    pub(crate) fn resume(&self, request: u64, fallback_origin: PaletteMainOrigin) -> bool {
        self.data
            .lock()
            .map(|mut data| {
                if data.revision != request {
                    return false;
                }
                match data.presentation {
                    Some(PalettePresentation::Pending) => {
                        data.presentation = Some(PalettePresentation::Active(fallback_origin));
                    }
                    Some(PalettePresentation::Active(_)) => {}
                    None => return false,
                }
                true
            })
            .unwrap_or(false)
    }

    pub(crate) fn complete(&self, request: u64) -> Option<PaletteDismissal> {
        let mut data = self.data.lock().ok()?;
        if data.revision != request {
            return None;
        }
        Some(match data.presentation.take() {
            Some(PalettePresentation::Active(origin)) => dismissal_after_palette(origin),
            Some(PalettePresentation::Pending) | None => PaletteDismissal::None,
        })
    }

    pub(crate) fn cancel(&self) {
        if let Ok(mut data) = self.data.lock() {
            Self::advance(&mut data);
            data.presentation = None;
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.data
            .lock()
            .map(|data| data.presentation.is_some())
            .unwrap_or(false)
    }
}
