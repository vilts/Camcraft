{
  "general": { "model": "Tormach 24R", "vendor": "Camcraft", "kind": "router" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -6.7, "max": 6.7 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -55.75, "max": 55.75 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -24.75, "max": 24.75 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 10000.0, "max_speed": 24000.0, "power": 1.5,
            "rigid_tapping": false } }
      ] }
    ] } },
  "controller": { "post": "Tormach PathPilot", "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 200.0, "max_rapid_speed": 200.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 200.0, "max_rapid_speed": 200.0 },
    "Z": { "coordinate": 2, "max_normal_speed": 200.0, "max_rapid_speed": 200.0 }
  } } }
}
