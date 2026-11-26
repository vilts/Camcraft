{
  "general": { "model": "Ooznest WorkBee 1000x1500 (Estlcam)", "vendor": "Camcraft", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -88.0, "max": 88.0 }, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -770.0, "max": 770.0 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -1200.0, "max": 1200.0 } }
          ] }
        ] },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "power": 2.2 } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0, "max_normal_speed": 2500.0, "max_rapid_speed": 2500.0 },
    "Y": { "coordinate": 1, "max_normal_speed": 2500.0, "max_rapid_speed": 2500.0 },
    "Z": { "coordinate": 2 }
  } } }
}
