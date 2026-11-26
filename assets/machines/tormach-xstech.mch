{
  "general": { "model": "Tormach xsTECH", "vendor": "Camcraft", "kind": "router" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -2.4, "max": 2.4 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -6.3, "max": 6.3 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -8.6, "max": 8.6 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 5000.0, "max_speed": 15000.0, "power": 0.08,
            "rigid_tapping": false } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 120.0, "max_rapid_speed": 120.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 120.0, "max_rapid_speed": 120.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 120.0, "max_rapid_speed": 120.0 }
  } } }
}
