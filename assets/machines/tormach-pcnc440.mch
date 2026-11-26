{
  "general": { "model": "Tormach PCNC 440", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -10.0, "max": 10.0 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -6.25, "max": 6.25 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -10.0, "max": 10.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 300.0, "max_speed": 10000.0, "power": 0.56,
            "ranges": [{ "min_speed": 300.0, "max_speed": 4500.0 }, { "min_speed": 650.0, "max_speed": 10000.0 }],
            "rigid_tapping": false },
          "tool_station": {
            "coolants": [
              { "name": "Flood", "mCode": "M8" },
              { "name": "Mist", "mCode": "M7" }
            ] } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 135.0, "max_rapid_speed": 135.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 135.0, "max_rapid_speed": 135.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 110.0, "max_rapid_speed": 110.0 }
  } } },
  "tooling": { "spindle_mount": "TTS", "default_holder": "TTS ER20",
    "mount_source": "https://www.tormach.com/tormach_mills/ - PCNC and M machines have an R8 collet in the spindle and use TTS tool holders" }
}
