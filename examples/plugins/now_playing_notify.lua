-- Now Playing Desktop Notifier Plugin for Tunotron
-- Place this script in ~/.config/tunotron/plugins/now_playing_notify.lua

local plugin = {}

plugin.manifest = {
    id = "org.tunotron.nowplaying.notify",
    name = "Now Playing Desktop Notifier",
    version = "0.1.0",
    description = "Dispatches desktop notifications when tracks change or via keybind",
    capabilities = {
        "KeyBind",
        "Notify"
    },
    keybinds = {
        ["N"] = "show_now_playing"
    }
}

plugin.current_track = nil

function plugin.on_load()
    tunotron.log("Now Playing Notifier loaded. Press 'N' to show current track notification.")
end

function plugin.on_event(event)
    if event.type == "TrackChanged" then
        plugin.current_track = event
        local summary = event.title or "Unknown Track"
        local body = string.format("%s\n%s", event.artist or "Unknown Artist", event.album or "Unknown Album")
        if event.duration_sec and event.duration_sec > 0 then
            local mins = math.floor(event.duration_sec / 60)
            local secs = math.floor(event.duration_sec % 60)
            body = string.format("%s [%02d:%02d]", body, mins, secs)
        end

        return {
            action = "Notify",
            summary = summary,
            body = body
        }
    elseif event.type == "PlaybackStopped" then
        plugin.current_track = nil
    end
    return {}
end

function plugin.on_action(name, payload)
    if name == "show_now_playing" then
        if plugin.current_track then
            local t = plugin.current_track
            local summary = t.title or "Unknown Track"
            local body = string.format("%s\n%s", t.artist or "Unknown Artist", t.album or "Unknown Album")
            return {
                action = "Notify",
                summary = summary,
                body = body
            }
        else
            return {
                action = "Notify",
                summary = "Tunotron",
                body = "No track currently playing"
            }
        end
    end
    return {}
end

function plugin.on_unload()
    tunotron.log("Now Playing Notifier unloaded.")
end

return plugin
