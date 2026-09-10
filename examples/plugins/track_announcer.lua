-- Track Announcer Example Plugin for Tunotron
-- Place this script in ~/.config/tunotron/plugins/track_announcer.lua

local plugin = {}

plugin.manifest = {
    id = "org.tunotron.announcer",
    name = "Track Announcer",
    version = "0.1.0",
    description = "Logs track information and demonstrates capability-checked playback actions",
    capabilities = {
        "PlaybackControl"
    }
}

-- Lifecycle hook: called when plugin is loaded into Tunotron
function plugin.on_load()
    tunotron.log("Track Announcer plugin loaded! Running Tunotron v" .. tunotron.version)
end

-- Event hook: called on domain state changes (e.g. track change, play state change)
function plugin.on_event(event)
    if event.type == "TrackChanged" then
        tunotron.log(string.format(
            "Now Playing: %s - %s (%.0fs)",
            event.artist or "Unknown Artist",
            event.title or "Unknown Title",
            event.duration_sec or 0
        ))

        -- Plugins with PlaybackControl capability can emit actions back to Tunotron.
        -- For instance, returning an empty list or specific actions:
        return {}
    end

    return {}
end

-- Action hook: called when a custom plugin action is invoked
function plugin.on_action(name, payload)
    tunotron.log("Received custom action: " .. name)
    return {}
end

-- Lifecycle hook: called when Tunotron unloads the plugin
function plugin.on_unload()
    tunotron.log("Track Announcer plugin unloaded.")
end

return plugin
