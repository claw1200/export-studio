use rb_device::settings::{
    self, DevSetting, DeviceSettings, KeyDisplay, OverviewWaveform, WaveformColor, WaveformPosition,
};
use serde_json::{json, Value};

use crate::library::failure;

pub(crate) fn view(settings: &DeviceSettings) -> Value {
    let display = settings.dev.clone().unwrap_or_default();
    json!({
        "snapshot": format!("{settings:?}"),
        "hasDisplaySettings": settings.dev.is_some(),
        "display": {
            "overview": match display.overview { OverviewWaveform::Half => "half", OverviewWaveform::Full => "full" },
            "color": match display.color { WaveformColor::Blue => "blue", WaveformColor::Rgb => "rgb", WaveformColor::TriBand => "3band" },
            "keyDisplay": match display.key_display { KeyDisplay::Classic => "classic", KeyDisplay::Alphanumeric => "alphanumeric" },
            "position": match display.position { WaveformPosition::Center => "center", WaveformPosition::Left => "left" },
        },
        "library": settings.library.as_ref().map(|library| json!({
            "deviceName": library.device_name,
            "backgroundColor": library.background_color_type,
            "categories": slots(&library.categories),
            "sorts": slots(&library.sorts),
            "subColumn": library.sub_column,
            "colors": library.colors.iter().map(|color| json!({ "id": color.id, "name": color.name })).collect::<Vec<_>>(),
        })),
        "deviceLibraryBackground": settings.device_library_background,
        "hasDeviceLibrary": settings.has_device_library,
        "hasOneLibrary": settings.has_one_library,
    })
}

fn slots(slots: &[settings::MenuSlot]) -> Vec<Value> {
    slots
        .iter()
        .map(|slot| {
            json!({
                "id": slot.id, "menuItem": slot.menu_item, "name": slot.name,
                "sequence": slot.seq, "visible": slot.visible,
            })
        })
        .collect()
}

fn invalid() -> napi::Error {
    failure("Invalid device settings")
}

fn text(value: &Value) -> napi::Result<&str> {
    value
        .as_str()
        .filter(|text| !text.contains('\0') && text.len() <= 255)
        .ok_or_else(invalid)
}

fn color(value: &Value) -> napi::Result<i64> {
    value
        .as_i64()
        .filter(|color| (0..=8).contains(color))
        .ok_or_else(invalid)
}

fn edit_slots(current: &mut [settings::MenuSlot], value: &Value) -> napi::Result<()> {
    let rows = value.as_array().ok_or_else(invalid)?;
    if rows.len() != current.len() {
        return Err(invalid());
    }
    let mut ids = std::collections::HashSet::new();
    for row in rows {
        let id = row["id"].as_i64().ok_or_else(invalid)?;
        if !ids.insert(id) {
            return Err(invalid());
        }
        let slot = current
            .iter_mut()
            .find(|slot| slot.id == id)
            .ok_or_else(invalid)?;
        slot.visible = row["visible"].as_bool().ok_or_else(invalid)?;
        slot.seq = row["sequence"]
            .as_i64()
            .filter(|seq| *seq >= 0 && *seq <= 1000)
            .ok_or_else(invalid)?;
    }
    Ok(())
}

pub(crate) fn edited(current: &DeviceSettings, value: &Value) -> napi::Result<DeviceSettings> {
    if value["snapshot"] != view(current)["snapshot"] {
        return Err(failure(
            "Device settings changed; reload them before saving",
        ));
    }
    let mut next = current.clone();
    let display = &value["display"];
    let mut dev: DevSetting = current.dev.clone().unwrap_or_default();
    dev.overview = match display["overview"].as_str() {
        Some("half") => OverviewWaveform::Half,
        Some("full") => OverviewWaveform::Full,
        _ => return Err(invalid()),
    };
    dev.color = match display["color"].as_str() {
        Some("blue") => WaveformColor::Blue,
        Some("rgb") => WaveformColor::Rgb,
        Some("3band") => WaveformColor::TriBand,
        _ => return Err(invalid()),
    };
    dev.key_display = match display["keyDisplay"].as_str() {
        Some("classic") => KeyDisplay::Classic,
        Some("alphanumeric") => KeyDisplay::Alphanumeric,
        _ => return Err(invalid()),
    };
    dev.position = match display["position"].as_str() {
        Some("center") => WaveformPosition::Center,
        Some("left") => WaveformPosition::Left,
        _ => return Err(invalid()),
    };
    if current.dev.is_some() || display != &view(current)["display"] {
        next.dev = Some(dev);
    }
    if let Some(library) = &mut next.library {
        let input = &value["library"];
        library.device_name = text(&input["deviceName"])?.to_owned();
        library.background_color_type = color(&input["backgroundColor"])?;
        edit_slots(&mut library.categories, &input["categories"])?;
        edit_slots(&mut library.sorts, &input["sorts"])?;
        library.sub_column = if input["subColumn"].is_null() {
            None
        } else {
            let id = input["subColumn"].as_i64().ok_or_else(invalid)?;
            if !library.sorts.iter().any(|slot| slot.menu_item == id) {
                return Err(invalid());
            }
            Some(id)
        };
        let colors = input["colors"].as_array().ok_or_else(invalid)?;
        if colors.len() != library.colors.len() {
            return Err(invalid());
        }
        for (original, edited) in library.colors.iter_mut().zip(colors) {
            if edited["id"].as_i64() != Some(original.id) {
                return Err(invalid());
            }
            original.name = text(&edited["name"])?.to_owned();
        }
    } else if !value["library"].is_null() {
        return Err(invalid());
    }
    if current.device_library_background.is_some() {
        next.device_library_background = Some(color(&value["deviceLibraryBackground"])? as u8);
    } else if !value["deviceLibraryBackground"].is_null() {
        return Err(invalid());
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> DeviceSettings {
        DeviceSettings {
            dev: None,
            library: Some(settings::StickSettings::default()),
            device_library_background: Some(0),
            has_device_library: true,
            has_one_library: true,
        }
    }

    #[test]
    fn unchanged_view_does_not_create_display_file() {
        let original = settings();
        assert_eq!(edited(&original, &view(&original)).unwrap(), original);
    }

    #[test]
    fn edits_and_validation() {
        let original = settings();
        let mut value = view(&original);
        value["display"]["keyDisplay"] = json!("alphanumeric");
        value["library"]["subColumn"] = Value::Null;
        value["library"]["colors"][0]["name"] = json!("Favorites");
        let next = edited(&original, &value).unwrap();
        assert_eq!(next.dev.unwrap().key_display, KeyDisplay::Alphanumeric);
        assert_eq!(next.library.unwrap().colors[0].name, "Favorites");
        value["library"]["categories"][0]["id"] = json!(-1);
        assert!(edited(&original, &value).is_err());
        value = view(&original);
        value["display"]["keyDisplay"] = json!("invalid");
        assert!(edited(&original, &value).is_err());
        value = view(&original);
        value["snapshot"] = json!("stale");
        assert!(edited(&original, &value).is_err());
    }
}
