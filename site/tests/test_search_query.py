"""Provenance: CamCraft docs search URL behaviour for issue #2329."""

import json
from pathlib import Path
import subprocess
import unittest


SITE = Path(__file__).resolve().parents[1]
HARNESS = r"""
const fs = require("node:fs");
const vm = require("node:vm");
const source = fs.readFileSync("templates/base.html", "utf8");
const script = source.match(/<script>\s*([\s\S]*?)<\/script>/)[1];
function element(tag) {
  return {
    tag, value: "", hidden: true, children: [], attributes: {},
    set innerHTML(value) { this.children = []; },
    setAttribute(name, value) { this.attributes[name] = value; },
    appendChild(child) { this.children.push(child); },
    addEventListener() {}
  };
}
const input = element("input");
input.attributes["aria-expanded"] = "false";
const list = element("ul");
const form = element("form");
const document = {
  querySelector: () => form,
  getElementById: id => id === "site-search" ? input : list,
  createElement: element,
  addEventListener() {}
};
const context = {document, URLSearchParams, console,
                 location: {search: process.argv[1]}};
context.window = context;
vm.createContext(context);
vm.runInContext(fs.readFileSync("static/vendor/elasticlunr.min.js", "utf8"), context);
const index = context.elasticlunr(function () {
  this.setRef("id");
  this.addField("title");
  this.addField("description");
  this.addField("body");
});
index.addDoc({id: "/Camcraft/guide/work-offset/", title: "Work offset",
              description: "Set the work offset", body: "Work offset setup"});
context.searchIndex = index.toJSON();
vm.runInContext(script, context);
process.stdout.write(JSON.stringify({
  value: input.value, hidden: list.hidden,
  expanded: input.attributes["aria-expanded"],
  links: list.children.flatMap(li => li.children.filter(a => a.tag === "a")
                             .map(a => ({href: a.href, text: a.textContent})))
}));
"""


class SearchQuery(unittest.TestCase):
    def load(self, query):
        result = subprocess.run(
            ["node", "-e", HARNESS, query], cwd=SITE,
            capture_output=True, text=True, check=True,
        )
        return json.loads(result.stdout)

    def test_query_on_load_opens_matching_results(self):
        for query in ("?q=work+offset", "?q=work%20offset"):
            with self.subTest(query=query):
                result = self.load(query)
                self.assertEqual(result["value"], "work offset")
                self.assertFalse(result["hidden"])
                self.assertEqual(result["expanded"], "true")
                self.assertEqual(result["links"], [{
                    "href": "/Camcraft/guide/work-offset/", "text": "Work offset",
                }])

    def test_no_query_keeps_search_closed(self):
        for query in ("", "?other=work+offset"):
            with self.subTest(query=query):
                self.assertEqual(self.load(query), {
                    "value": "", "hidden": True, "expanded": "false", "links": [],
                })

    def test_empty_query_keeps_search_closed(self):
        for query in ("?q=", "?q=++"):
            with self.subTest(query=query):
                result = self.load(query)
                self.assertTrue(result["hidden"])
                self.assertEqual(result["expanded"], "false")
                self.assertEqual(result["links"], [])


if __name__ == "__main__":
    unittest.main()
