-- Lyrics Viewer Plugin for Tunotron
-- Place this script in ~/.config/tunotron/plugins/lyrics_viewer.lua

local plugin = {}

plugin.manifest = {
    id = "org.tunotron.lyrics",
    name = "Local Lyrics Viewer",
    version = "0.1.0",
    description = "Displays synchronized or plain text lyrics (.lrc/.txt) for the playing track",
    capabilities = {
        "KeyBind",
        "UiOverlay",
        "FsJailRead"
    },
    keybinds = {
        ["y"] = "toggle_lyrics"
    }
}

plugin.current_track = nil

function plugin.on_load()
    tunotron.log("Local Lyrics Viewer loaded. Press 'y' while playing to view lyrics.")
end

function plugin.on_event(event)
    if event.type == "TrackChanged" then
        plugin.current_track = event
    elseif event.type == "PlaybackStopped" then
        plugin.current_track = nil
    end
    return {}
end

function plugin.on_action(name, payload)
    if name == "toggle_lyrics" then
        if not plugin.current_track or not plugin.current_track.path then
            return {
                action = "ShowModal",
                title = "Lyrics Viewer",
                content = "No track currently loaded.\nStart playing a song and press 'y'."
            }
        end

        local track = plugin.current_track
        local audio_path = track.path

        -- Try replacing audio extension with .lrc then .txt
        local lrc_path = audio_path:gsub("%.%w+$", ".lrc")
        local txt_path = audio_path:gsub("%.%w+$", ".txt")

        local lyrics_content = nil
        local found_path = nil

        if tunotron.file_exists(lrc_path) then
            lyrics_content = tunotron.read_file(lrc_path)
            found_path = lrc_path
        elseif tunotron.file_exists(txt_path) then
            lyrics_content = tunotron.read_file(txt_path)
            found_path = txt_path
        end

        if lyrics_content and lyrics_content ~= "" then
            return {
                action = "ShowModal",
                title = string.format("Lyrics: %s - %s", track.artist or "Unknown", track.title or "Track"),
                content = lyrics_content
            }
        else
            return {
                action = "ShowModal",
                title = "Lyrics Not Found",
                content = string.format(
                    "No local lyrics file found for:\n%s\n\nSearched locations:\n  - %s\n  - %s",
                    track.title or audio_path,
                    lrc_path,
                    txt_path
                )
            }
        end
    end

    return {}
end

function plugin.on_unload()
    tunotron.log("Local Lyrics Viewer unloaded.")
end

return plugin
