-- Track Announcer Example Plugin for Tunotron
-- Place this script in ~/.config/tunotron/plugins/track_announcer.lua

local plugin = {}

plugin.manifest = {
    id = "org.tunotron.announcer",
    name = "Track Announcer",
    version = "0.1.0",
    description = "Logs track information and demonstrates capability-checked playback actions",
    capabilities = {
        "PlaybackControl",
        "KeyBind",
        "UiOverlay"
    },
    keybinds = {
        ["i"] = "announce_current"
    }
}

plugin.last_track = nil

-- Lifecycle hook: called when plugin is loaded into Tunotron
function plugin.on_load()
    tunotron.log("Track Announcer plugin loaded! Running Tunotron v" .. tunotron.version)
end

-- Event hook: called on domain state changes (e.g. track change, play state change)
function plugin.on_event(event)
    if event.type == "TrackChanged" then
        plugin.last_track = event
        tunotron.log(string.format(
            "Now Playing: %s - %s (%.0fs)",
            event.artist or "Unknown Artist",
            event.title or "Unknown Title",
            event.duration_sec or 0
        ))
        return {}
    end

    return {}
end

-- Action hook: called when a custom plugin action or keybind is triggered
function plugin.on_action(name, payload)
    if name == "announce_current" then
        if plugin.last_track then
            local t = plugin.last_track
            local details = string.format(
                "🎵 Title:    %s\n👤 Artist:   %s\n💿 Album:    %s\n⏱️ Duration: %.0f sec\n📂 Path:     %s",
                t.title or "Unknown",
                t.artist or "Unknown",
                t.album or "Unknown",
                t.duration_sec or 0,
                t.path or "Unknown"
            )
            return {
                action = "ShowModal",
                title = "Track Inspector",
                content = details
            }
        else
            return {
                action = "ShowModal",
                title = "Track Inspector",
                content = "No track currently playing.\nSelect a song in the browser and press Enter."
            }
        end
    else
        tunotron.log("Received custom action: " .. name)
    end
    return {}
end

-- Lifecycle hook: called when Tunotron unloads the plugin
function plugin.on_unload()
    tunotron.log("Track Announcer plugin unloaded.")
end

return plugin
