{
  "general": {
    "model": "Doosan DNM 5700",
    "vendor": "DN Solutions",
    "kind": "mill",
    "description": "DN Solutions DNM 5700 VMC (Fanuc 0i-MF). Specs: DN Solutions DNM 4500/5700/6700 Catalog (X/Y/Z: 1020/570/510mm, rapid 36/36/30 m/min, 12000 RPM spindle 18.5kW, rigid tapping, 30-tool ATC). The plain 3-axis machine; the optional 4th-axis rotary table is the separate built-in Doosan DNM 5700 + rotary table (#1433)"
  },
  "kinematics": {
    "default": {
      "units": { "length": "mm", "angle": "degrees" },
      "parts": [
        { "id": "base", "name": "table", "type": "table", "parts": [
          { "id": "crl", "name": "X", "type": "linear", "travel": { "min": -1020.0, "max": 1020.0 }, "parts": [
            { "id": "sld", "name": "Y", "type": "linear", "travel": { "min": -570.0, "max": 570.0 } }
          ] },
          { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -510.0, "max": 510.0 }, "safe_retract": 0.0, "parts": [
            { "id": "hed", "name": "head", "type": "head",
              "spindle": { "min_speed": 40.0, "max_speed": 12000.0, "power": 18.5,
                           "body_diameter": 150.0, "body_length": 250.0, "rigid_tapping": true },
              "tool_station": { "max_tool_length": 300.0, "max_tool_diameter": 80.0,
                "coolants": [
                  { "name": "Flood", "mCode": "M8" },
                  { "name": "Mist", "mCode": "M7" },
                  { "name": "Through-tool", "mCode": "M88" }
                ] } }
          ] }
        ] }
      ]
    }
  },
  "controller": {
    "synced_configuration": {
      "parts": {
        "X": { "coordinate": 0, "max_normal_speed": 15000.0, "max_rapid_speed": 36000.0 },
        "Y": { "coordinate": 1, "max_normal_speed": 15000.0, "max_rapid_speed": 36000.0 },
        "Z": { "coordinate": 2, "max_normal_speed": 15000.0, "max_rapid_speed": 30000.0 }
      }
    }
  },
  "tooling": { "number_of_tools": 30 }
}
