-- Sleep Timer Plugin for Tunotron
-- Place this script in ~/.config/tunotron/plugins/sleep_timer.lua

local plugin = {}

plugin.manifest = {
    id = "org.tunotron.sleeptimer",
    name = "Sleep Timer",
    version = "0.1.0",
    description = "Automatically pauses playback after a configurable countdown timer",
    capabilities = {
        "KeyBind",
        "UiOverlay"
    },
    keybinds = {
        ["T"] = "toggle_timer"
    }
}

-- Default countdown duration: 15 minutes (900 seconds)
local DEFAULT_DURATION_SEC = 15 * 60
plugin.remaining_sec = nil

function plugin.on_load()
    tunotron.log("Sleep Timer plugin loaded. Press 'T' to start/cancel a 15-minute sleep timer.")
end

function plugin.on_action(name, payload)
    if name == "toggle_timer" then
        if plugin.remaining_sec == nil then
            plugin.remaining_sec = DEFAULT_DURATION_SEC
            local mins = math.floor(DEFAULT_DURATION_SEC / 60)
            return {
                action = "ShowModal",
                title = "Sleep Timer Activated",
                content = string.format(
                    "Playback will automatically pause in %d minutes (%d seconds).\n\nPress 'Z' again at any time to cancel.",
                    mins,
                    DEFAULT_DURATION_SEC
                )
            }
        else
            plugin.remaining_sec = nil
            return {
                action = "ShowModal",
                title = "Sleep Timer Cancelled",
                content = "The sleep timer has been turned off."
            }
        end
    end
    return {}
end

-- 1 Hz monotonic playback heartbeat hook
function plugin.on_tick(position, duration)
    if plugin.remaining_sec ~= nil then
        plugin.remaining_sec = plugin.remaining_sec - 1
        if plugin.remaining_sec <= 0 then
            plugin.remaining_sec = nil
            return {
                { action = "TogglePause" },
                {
                    action = "ShowModal",
                    title = "Sleep Timer Expired",
                    content = "The sleep timer countdown completed.\nPlayback has been paused. Sleep well!"
                }
            }
        end
    end
    return {}
end

function plugin.on_event(event)
    if event.type == "PlaybackStopped" then
        -- Cancel running timer if playback completely stopped
        plugin.remaining_sec = nil
    end
    return {}
end

function plugin.on_unload()
    tunotron.log("Sleep Timer plugin unloaded.")
end

return plugin
