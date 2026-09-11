-- 3D Spatial Audio Studio Extension for Tunotron
-- Multi-axis binaural orbit simulator, room acoustics & real-time radar visualizer.
-- Place in ~/.config/tunotron/plugins/spatial_audio.lua

local plugin = {}

plugin.manifest = {
    id = "org.tunotron.spatial_audio",
    name = "3D Spatial Audio Studio",
    version = "1.0.0",
    description = "Multi-axis 3D/7D/8D binaural revolving spatial audio simulator with real-time soundstage radar",
    capabilities = {
        "PlaybackControl",
        "UiOverlay",
        "PersistentStorage",
        "KeyBind"
    },
    keybinds = {
        ["ctrl+s"] = "toggle_spatializer"
    }
}

-- Default Configuration & Presets
local PRESETS = {
    ["Default 8D Orbit"] = {
        speed = 0.15,
        elevation = 0.0,
        trajectory = "Circular (360°)",
        direction = "Clockwise",
        room = "Living Room",
        reverb = 20.0,
        width = 160.0
    },
    ["Concert Dome"] = {
        speed = 0.08,
        elevation = 30.0,
        trajectory = "Circular (360°)",
        direction = "Clockwise",
        room = "Concert Hall",
        reverb = 45.0,
        width = 200.0
    },
    ["Ear-to-Ear Whisper"] = {
        speed = 0.25,
        elevation = 0.0,
        trajectory = "Ear-to-Ear (Left-Right)",
        direction = "Clockwise",
        room = "Dry Studio",
        reverb = 5.0,
        width = 220.0
    },
    ["Cathedral Echo"] = {
        speed = 0.05,
        elevation = 45.0,
        trajectory = "Figure-8 (Infinity)",
        direction = "Counter-Clockwise",
        room = "Cathedral",
        reverb = 75.0,
        width = 250.0
    },
    ["Cosmic Swirl"] = {
        speed = 0.35,
        elevation = -15.0,
        trajectory = "Spiral (In-Out)",
        direction = "Clockwise",
        room = "Cosmic Void",
        reverb = 60.0,
        width = 280.0
    },
    ["Binaural Pendulum"] = {
        speed = 0.18,
        elevation = 0.0,
        trajectory = "Pendulum (Front-Back)",
        direction = "Clockwise",
        room = "Club Stage",
        reverb = 30.0,
        width = 180.0
    }
}

local PRESET_NAMES = {
    "Default 8D Orbit",
    "Concert Dome",
    "Ear-to-Ear Whisper",
    "Cathedral Echo",
    "Cosmic Swirl",
    "Binaural Pendulum",
    "Custom"
}

local TRAJECTORIES = {
    "Circular (360°)",
    "Figure-8 (Infinity)",
    "Spiral (In-Out)",
    "Pendulum (Front-Back)",
    "Ear-to-Ear (Left-Right)"
}

local DIRECTIONS = {
    "Clockwise",
    "Counter-Clockwise"
}

local ROOMS = {
    "Dry Studio",
    "Living Room",
    "Club Stage",
    "Concert Hall",
    "Cathedral",
    "Cosmic Void"
}

local ROOM_DELAYS = {
    ["Dry Studio"] = 15,
    ["Living Room"] = 45,
    ["Club Stage"] = 90,
    ["Concert Hall"] = 180,
    ["Cathedral"] = 420,
    ["Cosmic Void"] = 750
}

-- Active runtime state
local state = {
    enabled = false,
    selected_preset = "Default 8D Orbit",
    speed = 0.15,
    elevation = 0.0,
    trajectory = "Circular (360°)",
    direction = "Clockwise",
    room = "Living Room",
    reverb = 20.0,
    width = 160.0
}

local function find_index(list, val)
    for i, v in ipairs(list) do
        if v == val then
            return i - 1
        end
    end
    return 0
end

-- Builds dynamic ffmpeg lavfi filtergraph string for mpv
local function build_lavfi_filter()
    if not state.enabled then
        return ""
    end

    local filters = {}

    -- 1. Binaural Orbit / Pan Oscillation
    local pulsator_mode = "sine"
    if state.trajectory == "Figure-8 (Infinity)" then
        pulsator_mode = "triangle"
    elseif state.trajectory == "Ear-to-Ear (Left-Right)" then
        pulsator_mode = "square"
    end

    local offset_r = 0.5
    if state.direction == "Counter-Clockwise" then
        offset_r = 0.0
    end

    table.insert(filters, string.format(
        "apulsator=mode=%s:hz=%.2f:amount=0.92:offset_r=%.2f",
        pulsator_mode,
        math.max(0.01, state.speed),
        offset_r
    ))

    -- 2. Headphone Acoustic Crossfeed & Stereo Spread
    local spread_ratio = math.max(0.0, (state.width - 100.0) / 100.0)
    local stereo_delay = math.floor(15 + spread_ratio * 25)
    local stereo_feedback = string.format("%.2f", math.min(0.5, 0.2 + spread_ratio * 0.15))
    table.insert(filters, string.format(
        "stereowiden=delay=%d:feedback=%s:crossfeed=0.25:drymix=0.85",
        stereo_delay,
        stereo_feedback
    ))

    -- 3. Room Acoustics & Reverb Echo
    if state.reverb > 1.0 then
        local echo_delay = ROOM_DELAYS[state.room] or 60
        local echo_decay = string.format("%.2f", math.min(0.65, (state.reverb / 100.0) * 0.55))
        table.insert(filters, string.format(
            "aecho=0.85:0.80:%d:%s",
            echo_delay,
            echo_decay
        ))
    end

    -- 4. Psychoacoustic Pinna Elevation Notch Filter (~6 kHz notch/peak)
    if math.abs(state.elevation) > 2.0 then
        local gain = string.format("%.1f", (state.elevation / 90.0) * 5.0)
        table.insert(filters, string.format("equalizer=f=6200:t=q:w=1.8:g=%s", gain))
    end

    return "lavfi=[" .. table.concat(filters, ",") .. "]"
end

local function make_page_table()
    return {
        id = "spatial_audio",
        title = "3D Revolving Spatial Soundstage Studio",
        description = "Real-time binaural multi-axis orbit simulator and acoustic room spatializer",
        selected_field = 0,
        radar = {
            angle = 0.0,
            distance = 1.0,
            elevation = state.elevation,
            label = "Soundstage [Center]"
        },
        fields = {
            {
                type = "Toggle",
                id = "enabled",
                label = "Spatializer Engine",
                checked = state.enabled
            },
            {
                type = "Select",
                id = "preset",
                label = "Studio Preset",
                options = PRESET_NAMES,
                selected = find_index(PRESET_NAMES, state.selected_preset)
            },
            {
                type = "Slider",
                id = "speed",
                label = "Orbit Speed",
                value = state.speed,
                min = 0.01,
                max = 1.50,
                step = 0.02,
                unit = " Hz"
            },
            {
                type = "Slider",
                id = "elevation",
                label = "Orbit Elevation",
                value = state.elevation,
                min = -90.0,
                max = 90.0,
                step = 5.0,
                unit = "°"
            },
            {
                type = "Select",
                id = "trajectory",
                label = "Orbit Trajectory",
                options = TRAJECTORIES,
                selected = find_index(TRAJECTORIES, state.trajectory)
            },
            {
                type = "Select",
                id = "direction",
                label = "Orbit Direction",
                options = DIRECTIONS,
                selected = find_index(DIRECTIONS, state.direction)
            },
            {
                type = "Select",
                id = "room",
                label = "Room Simulation",
                options = ROOMS,
                selected = find_index(ROOMS, state.room)
            },
            {
                type = "Slider",
                id = "reverb",
                label = "Room Reverb Depth",
                value = state.reverb,
                min = 0.0,
                max = 100.0,
                step = 5.0,
                unit = "%"
            },
            {
                type = "Slider",
                id = "width",
                label = "Stereo Width Spread",
                value = state.width,
                min = 100.0,
                max = 300.0,
                step = 10.0,
                unit = "%"
            },
            {
                type = "Button",
                id = "save_preset",
                label = "Save Current Configuration to Disk"
            }
        }
    }
end

function plugin.on_load()
    -- Restore persisted state if available
    local saved, err = tunotron.state.get("spatial_state")
    if saved and type(saved) == "table" then
        if saved.enabled ~= nil then state.enabled = saved.enabled end
        if saved.selected_preset ~= nil then state.selected_preset = saved.selected_preset end
        if saved.speed ~= nil then state.speed = saved.speed end
        if saved.elevation ~= nil then state.elevation = saved.elevation end
        if saved.trajectory ~= nil then state.trajectory = saved.trajectory end
        if saved.direction ~= nil then state.direction = saved.direction end
        if saved.room ~= nil then state.room = saved.room end
        if saved.reverb ~= nil then state.reverb = saved.reverb end
        if saved.width ~= nil then state.width = saved.width end
    end

    tunotron.log("3D Spatial Audio Studio initialized (Press 2 or Tab to open studio page, Ctrl+S to toggle)")

    return {
        {
            action = "RegisterTab",
            id = "spatial_audio",
            title = "3D Spatial",
            shortcut = "2"
        },
        {
            action = "SetExtensionPage",
            page = make_page_table()
        }
    }
end

function plugin.on_action(name, payload)
    if name == "toggle_spatializer" then
        state.enabled = not state.enabled
        local msg = state.enabled and "3D Spatial Audio: Active" or "3D Spatial Audio: Bypassed"
        local filter_cmd = build_lavfi_filter()

        return {
            { action = "UpdateExtensionPageField", page_id = "spatial_audio", field_id = "enabled", value = state.enabled },
            { action = "SetAudioFilter", filter = filter_cmd },
            { action = "ShowToast", message = msg, duration_ms = 2000 }
        }
    end

    if name == "on_form_change" and payload then
        local field = payload.field
        local val = payload.value

        if field == "enabled" then
            state.enabled = (val == true)
        elseif field == "preset" then
            state.selected_preset = tostring(val)
            if PRESETS[state.selected_preset] then
                local p = PRESETS[state.selected_preset]
                state.speed = p.speed
                state.elevation = p.elevation
                state.trajectory = p.trajectory
                state.direction = p.direction
                state.room = p.room
                state.reverb = p.reverb
                state.width = p.width

                return {
                    { action = "SetExtensionPage", page = make_page_table() },
                    { action = "SetAudioFilter", filter = build_lavfi_filter() },
                    { action = "ShowToast", message = string.format("Applied Preset: %s", state.selected_preset), duration_ms = 1500 }
                }
            end
        elseif field == "speed" then
            state.speed = tonumber(val) or state.speed
            state.selected_preset = "Custom"
        elseif field == "elevation" then
            state.elevation = tonumber(val) or state.elevation
            state.selected_preset = "Custom"
        elseif field == "trajectory" then
            state.trajectory = tostring(val)
            state.selected_preset = "Custom"
        elseif field == "direction" then
            state.direction = tostring(val)
            state.selected_preset = "Custom"
        elseif field == "room" then
            state.room = tostring(val)
            state.selected_preset = "Custom"
        elseif field == "reverb" then
            state.reverb = tonumber(val) or state.reverb
            state.selected_preset = "Custom"
        elseif field == "width" then
            state.width = tonumber(val) or state.width
            state.selected_preset = "Custom"
        elseif field == "save_preset" then
            tunotron.state.set("spatial_state", state)
            tunotron.state.save()
            return {
                { action = "ShowToast", message = "✓ Saved 3D Spatial Audio configuration to disk", duration_ms = 2500 }
            }
        end

        local actions = {
            { action = "SetAudioFilter", filter = build_lavfi_filter() }
        }

        if state.selected_preset == "Custom" then
            table.insert(actions, {
                action = "UpdateExtensionPageField",
                page_id = "spatial_audio",
                field_id = "preset",
                value = "Custom"
            })
        end

        return actions
    end

    return {}
end

-- Real-time 3D soundstage orbit animation and radar tracking hook (1 Hz tick calibration)
function plugin.on_tick(position, duration)
    if not state.enabled then
        return {
            { action = "ClearSlot", slot = "spatial" }
        }
    end

    -- Calculate acoustic angular orbit
    local speed = math.max(0.01, state.speed)
    local dir_mult = (state.direction == "Counter-Clockwise") and -1.0 or 1.0
    local raw_angle = (position * speed * 2.0 * math.pi * dir_mult) % (2.0 * math.pi)

    local heading_str = "Binaural Orbit"
    local deg = math.floor((raw_angle * 180.0 / math.pi) + 0.5) % 360

    if deg >= 315 or deg < 45 then
        heading_str = "Front [N]"
    elseif deg >= 45 and deg < 135 then
        heading_str = "Right Ear [E]"
    elseif deg >= 135 and deg < 225 then
        heading_str = "Behind [S]"
    else
        heading_str = "Left Ear [W]"
    end

    local slot_txt = string.format("[3D: %.2fHz %s]", state.speed, state.room)

    return {
        {
            action = "UpdateRadar",
            page_id = "spatial_audio",
            radar = {
                angle = raw_angle,
                distance = 1.0,
                elevation = state.elevation,
                label = string.format("Orbit [%s]", heading_str)
            }
        },
        {
            action = "SetSlot",
            slot = "spatial",
            content = slot_txt
        }
    }
end

return plugin
