+++
title = "Use AI with CamCraft"
description = "Set up AI Chat or an external MCP agent, make a CAM job, undo edits and confirm posting."
weight = 3
aliases = ["guide/ai/"]
+++

Use **AI Chat** for a model inside the app, or connect an external MCP
agent to the open window. Both use the document commands and safety checks.
An AI answer is a proposal. Inspect its tool cards, geometry and toolpaths
before you accept a program.

## Set up the in-app panel

Use the native desktop app. Open **View > Panels > AI Chat**.
Choose **Provider**, enter a **Model** that your provider or local runtime
can serve, and type in **Prompt**. Select **Send** to start a turn.
**Cancel** stops further work; inspect the completed tool cards afterward.

### Cloud providers

An API key is a secret that lets CamCraft use your provider account.
Keep it secret, like a password. Do not put it in a chat prompt or a
project file. Cloud use can cost money, separate from a chat subscription.

Choose a cloud **Provider**, then select **Set API key...**. Paste the key
into the password field and save it. CamCraft stores it in the OS keychain.
The field clears after save, cancel, or a provider change. CamCraft never
shows the stored key. To replace it, use **Set API key...** again.

The status appears when you choose a provider. **Ready (key from keychain)**
or **Ready (key from environment)** means that CamCraft found a key.
It does not prove that the key works or that the account has credits.
**No key - Set API key...** means that you must add a key. **Test** stays
disabled until CamCraft finds a key.
Select **Test** to check access with a small request to the selected model.
CamCraft removes spaces around the **Model** text. An empty **Model** uses the
provider's default model. The result names the model that was tested.
A failed test shows the cause (for example a rejected key, an unknown model,
billing, a rate limit or the network), the model and host, what to try, and
the provider's error text. The key is never shown. The app log gets the same
text, so you can add it to a bug report.
The provider can bill this request. Clear **Offline only** before a cloud test.
**Remove** deletes the keychain entry only. It does not revoke the key at
the provider or remove an environment variable.

#### Get an Anthropic key

1. Sign in to the [Anthropic console](https://platform.claude.com/settings/keys), then open **Settings > API keys**.
2. Select **Create key**. Name it, for example, **CamCraft on my Mac**. This helps you revoke only this key later.
   If the console asks **Where does your code run?**, choose **Somewhere else - Use an API key**. The other choices do not give a key.
3. Copy the key at once. It starts with `sk-ant-`, and the console shows it only once.
4. Paste it into **Set API key...** in CamCraft and save it.
5. Keep it secret. Delete it on the same API keys page to revoke it.
6. Check [billing](https://platform.claude.com/settings/billing) and [spend limits](https://platform.claude.com/settings/spend-limits) before use. Cloud requests use credits.

See Anthropic's [API key instructions](https://platform.claude.com/docs/en/get-api-key).

#### Get an OpenAI key

1. Sign in to the [OpenAI API keys page](https://platform.openai.com/api-keys).
2. Create a secret key. Name it, for example, **CamCraft on my Mac**.
3. Copy it at once. Save the secret when the page shows it.
4. Paste it into **Set API key...** in CamCraft and save it. Both OpenAI presets use the same key.
5. Keep it secret. Delete it on the same API keys page to revoke it.
6. Check [API billing](https://platform.openai.com/settings/organization/billing/overview) and [limits](https://platform.openai.com/settings/organization/limits) before use. A ChatGPT subscription does not supply API credits.

#### Get a Gemini key

A Gemini app subscription does not include an API key. Create the key in
Google AI Studio with your Google account.

1. Sign in to [Google AI Studio](https://aistudio.google.com/api-keys).
2. Create an API key for your Google Cloud project. Give it a name that identifies this computer.
3. Copy the key and keep it secret.
4. Paste it into **Set API key...** in CamCraft and save it.
5. Delete the key on the same API keys page to revoke it.
6. Read [Gemini API billing](https://ai.google.dev/gemini-api/docs/billing). Check the project's billing and quotas before use.

#### Environment keys and model selection

| Provider choice | Credential | Setup |
|---|---|---|
| Anthropic (Cloud) | ANTHROPIC_API_KEY | Select this preset and enter an available Claude model ID in **Model**. |
| OpenAI Responses (Cloud) | OPENAI_API_KEY | Select this preset for the Responses API. Enter an available GPT or Codex model ID. |
| OpenAI Chat (Cloud) | OPENAI_API_KEY | Select this preset for the Chat Completions API. Enter a model ID supported by that API. |
| Gemini (Cloud) | GEMINI_API_KEY | Select this preset and enter an available Google Gemini model ID. |

Clear **Offline only** to use a cloud provider. Send a read-only request:
“Read the current job and tell me which setup is active. Change nothing.”
Check that the returned setup matches the Browser.

The app first reads the nonempty environment variable, then the OS keychain.
An environment key overrides a stored key, including a replacement you save.
Remove the variable from the app's launch environment to use the stored key.
Restart the app after changing its launch environment. An app started from
the Dock normally does not receive variables from your shell.

The keychain service is **camcraft-ai**. The accounts are **anthropic**,
**openai** and **gemini**. Native storage uses macOS Keychain, Windows
Credential Manager, or the freedesktop Secret Service on Linux.
CamCraft reports a keychain access failure by name.
Keys are not stored in the CAM project or the preferences file.

#### OS keychain permission prompts

On the first **Send** after you store a key, macOS can say that CamCraft
wants to use confidential information stored in **camcraft-ai**. It can
ask for the **login** keychain password. This is normally your Mac login
password. Enter it in the macOS prompt, never in CamCraft's key field.
Choose **Always Allow** to permit later reads. **Allow** permits one read,
so macOS can ask again on the next **Send**. A readiness check or **Test**
can also cause this prompt.

CamCraft is not code-signed. After an update, macOS can ask again because
the application changed. This is expected.

Windows stores the key in **Credential Manager**. Windows can ask for
permission or account authentication, or it can allow access without a
prompt. Linux uses **Secret Service**, such as a desktop keyring. The
desktop can ask you to unlock the keyring or approve access. If no Secret
Service is available, CamCraft reports that storage is unavailable.

### Ollama and offline-only mode

Install Ollama from the [download page](https://ollama.com/download), then
start it. Follow the [quickstart](https://docs.ollama.com/quickstart) to
install a model. For example, run `ollama pull qwen2.5:14b` in Terminal.
Choose a model that fits your computer's memory.

Ollama runs on http://localhost:11434. The panel uses its OpenAI-compatible endpoint
at http://localhost:11434/v1.
CamCraft does not start Ollama. Start the Ollama app or run `ollama serve`
first. If the Ollama app is in your menu bar, the server is already running.
If the app is not running, start the server with `ollama serve`.
Check it with `curl http://localhost:11434/api/version` or `ollama list`.
On Linux, `systemctl status ollama` checks the installed service.
The status names an unavailable server or an uninstalled model before you
send a prompt. While the server is unavailable, CamCraft checks it again
every 5 seconds. **Test** shows **Testing...** and then a passed or failed result. Start Ollama if the server is unavailable. Install the exact
model named in **Model** if it is missing, then select **Test** again.

1. Choose **Offline local (Ollama)**.
2. Enter the exact installed model name in **Model**.
3. Tick **Offline only**.
4. Send the read-only request above.

Set these environment variables before `ollama serve`:

| Variable | Effect |
|---|---|
| `OLLAMA_HOST=127.0.0.1:11500` | Changes the port. The client would need `http://127.0.0.1:11500/v1`. The panel has no endpoint field yet; keep the default address for CamCraft. |
| `OLLAMA_HOST=0.0.0.0:11434` | Exposes the server to other computers. Anyone on that network can use it. CamCraft's local panel connects only to loopback; it cannot connect to a remote GPU server yet. |
| `OLLAMA_CONTEXT_LENGTH=32768` | Gives the tool list more context space. Use it if the computer has enough memory. A small context can truncate the tools. |
| `OLLAMA_KEEP_ALIVE=30m` | Keeps the model loaded between messages. This avoids a reload on each Send. |

In native-tools mode, CamCraft first asks Ollama whether the model supports
tools. If it does not, choose **Local Prompt-JSON** for proposed tool calls,
or **Local Q&A (Read-Only)** for text answers. Both presets use Ollama too.
Prompt-JSON asks for confirmation before each document edit. Read-only Q&A
runs no tools and displays a banner.

**Offline only** is saved in the app preferences. It blocks cloud presets
before a request leaves the app. The HTTP client also restricts the
connection to loopback endpoints. It does not configure an external agent:
set that agent's model and network policy separately.

### Other local runtimes: current limit

The provider library defines these endpoints:

| Runtime | Endpoint | Setup limit in the shipped panel |
|---|---|---|
| Unsloth Studio | http://127.0.0.1:8888/v1 | The runtime's base address is http://127.0.0.1:8888. There is no selectable panel preset. |
| LM Studio | http://localhost:1234/v1 | There is no selectable panel preset. |
| llama-server | http://127.0.0.1:8080/v1 | The library's tool-calling preset expects the runtime to start with **--jinja**. There is no selectable panel preset. |

The panel has no endpoint field. All three local panel choices connect to
Ollama. Starting Unsloth Studio, LM Studio or llama-server does not make the
panel connect to it. There is no supported panel setup for these runtimes
in this version. Use a compatible external MCP agent if it supports your
runtime. Configure that connection in the external agent. The **--jinja**
flag belongs to llama-server, not to CamCraft.

## Connect an external agent

Install the chosen agent and make **camcraft-cli** available on its PATH,
or use the full executable path in its server configuration.
For a job you can watch and inspect, start the app:

```text
camcraft-ui --mcp-port 7777
```

Add an MCP server named **camcraft** in the external client's stdio server
settings. The server entry needs this command and these arguments:

```json
{
  "command": "camcraft-cli",
  "args": ["mcp", "--connect", "127.0.0.1:7777"]
}
```

| External client | Setup steps |
|---|---|
| Claude Code | Add the stdio server entry above in the client's MCP settings. Restart or reconnect the client, then enable its CamCraft tools. |
| Codex CLI | Add the same command and argument array as a stdio MCP server in the client's MCP configuration. Reconnect and enable the server. |
| Gemini CLI | Register the same stdio MCP server in the client's settings. Reconnect and enable its tools. |
| Antigravity CLI | Use its MCP server settings to register the same command and arguments. Reconnect and enable the server. |

The client owns its configuration format and tool permissions. The JSON
above shows the server entry, not a complete client configuration file.
Use the client's current instructions for the surrounding file format.
If a client version has no stdio MCP support, it cannot use this bridge.

The process that the client starts is equivalent to:

```text
camcraft-cli mcp --connect 127.0.0.1:7777
```

Ask the agent to list tools, call `get_job`, and describe the active setup.
Confirm that it sees the open document before asking for an edit.
The app listens on loopback only and serves one client at a time.
Disconnect the first client before attaching another. An idle client is
disconnected after five minutes without a complete request.

### Headless scripting

For an independent document with no app window, register the same server
with just the argument **mcp**, or start:

```text
camcraft-cli mcp
```

The server reads one JSON-RPC message per line from standard input.
It writes protocol replies to standard output. A script must keep the
process alive for the whole job, send requests in order, and check each
reply before it sends a dependent request.

Initialize the session, send the initialized notification, then request
the live schema:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cam-job","version":"1"}}}
```

```json
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

The examples below are individual `tools/call` requests.
An external agent normally makes them for you. A headless script can send
them directly. Headless mode has no panel confirmation prompt, so the
script's operator must control file paths and posting. Save the project
before the process ends if you need to keep its document.

## Four worked CAM tasks

Use a STEP bracket with a planar pocket floor for this example.
Check its units, alignment and dimensions before you start.
The example stock is 100 × 60 × 25 mm, centered in XY, from Z=0 to Z=25.
It must enclose your actual part. Adjust these dimensions and the WCS
to your blank and fixture. Use a new document for the exercise.

Each JSON block below is a separate request. Replace the model path,
catalog name and feature ID with real values from your session.
The requests show the tool arguments; they do not assert that a particular
bracket, cutter or machine is safe.

## 1. Import a model and create a setup and stock

Panel prompt: “Import bracket.step as bracket. Create a setup named Top.
Set a 100 by 60 by 25 mm box stock. Put the work origin at its top center.
Show me the result and stop before you add an operation.”

The import asks for file access. Inspect its path and select **Allow**,
or **Deny** if it is wrong.

```json
{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"import_model","arguments":{"path":"/absolute/path/bracket.step","name":"bracket"}}}
```

```json
{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"add_setup","arguments":{"name":"Top"}}}
```

```json
{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"set_stock","arguments":{"box":{"dx":100,"dy":60,"dz":25}}}}
```

```json
{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"set_wcs","arguments":{"stock_point":{"x":"center","y":"center","z":"max"}}}}
```

```json
{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"get_job","arguments":{}}}
```

Check the active setup, model placement, stock and WCS in the window.
A declared setup becomes active. Stock and WCS commands act on that setup.
A different model position needs a different stock placement or alignment;
the box dimensions alone do not align the part.

## 2. Pocket a picked face, generate toolpaths and read warnings

Pick the planar pocket floor in the viewport. Tick **Attach selection**
before sending: “Add a 2D Pocket named Pocket floor on this face. Use a
suitable catalog end mill. Generate it, verify it, and report every
warning. Do not post.”

Read the actual selection and inspect the available tools and parameters:

```json
{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"get_selection","arguments":{}}}
```

```json
{"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"list_features","arguments":{"model":"bracket","kinds":["faces"],"limit":100}}}
```

```json
{"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"list_tool_catalog","arguments":{}}}
```

```json
{"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"list_operation_types","arguments":{}}}
```

Use a real catalog name for a cutter that fits the pocket.
The displayed **2D Pocket** operation uses the kind **Pocket**.
Confirm its current parameters in the returned operation list.

```json
{"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"add_tool","arguments":{"catalog":"<catalog end mill name>","name":"Pocket cutter"}}}
```

```json
{"jsonrpc":"2.0","id":25,"method":"tools/call","params":{"name":"add_operation","arguments":{"kind":"Pocket","tool":"Pocket cutter","name":"Pocket floor","source":"bracket"}}}
```

Copy the selected face's **model** and **id** from **geometry_picks**.
Alternatively use the matching face row from `list_features`.
Replace the ID below; never guess one. Do not copy a display **name**
into the ID field. Headless mode has no viewport selection, so use the
feature list and inspect the intended geometry before generation.

```json
{"jsonrpc":"2.0","id":26,"method":"tools/call","params":{"name":"set_operation_geometry","arguments":{"operation":"Pocket floor","picks":[{"model":"bracket","id":"<face ID from this session>"}]}}}
```

```json
{"jsonrpc":"2.0","id":27,"method":"tools/call","params":{"name":"generate","arguments":{"name":"Pocket floor"}}}
```

```json
{"jsonrpc":"2.0","id":28,"method":"tools/call","params":{"name":"verify","arguments":{"name":"Pocket floor"}}}
```

Read every **findings** entry: **op**, **move**, **kind**, **message** and
**blocks_post**. Program-level findings can have no operation or move.
A tool call that completed successfully does not prove the program safe.
Resolve blocking findings and inspect the generated path over the part.
See [Reading a refusal](@/guide/reference/refusals.md).

## 3. Change a parameter and undo the turn

Read `get_job` and the kind's parameter schema first. Note the current
feed. In a separate panel turn ask: “Change Pocket floor feed to 300
mm/min. Make no other edit. Tell me its previous and new values.”

```json
{"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"edit_operation","arguments":{"name":"Pocket floor","params":{"feed":300}}}}
```

```json
{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"get_job","arguments":{}}}
```

Check the new value and stale status. Select **Undo Turn** on that
completed turn to restore its changes together. This button is available
only while the turn's edits remain the latest history entries. A later
manual edit disables it; do not expect it to overwrite later work.
Read the job again to confirm the original feed.

For an external agent, `undo` takes back the last command, not a named
chat turn. In this one-edit example, one call restores the feed:

```json
{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{"name":"undo","arguments":{}}}
```

Do not also send that call after using **Undo Turn**. It would undo an
earlier command. Regenerate and verify after a parameter change or undo
whenever the operation is stale.

## 4. Verify a program and post G-code with panel confirmation

Choose the correct machine and post for the real controller first.
Use the setup and machine pages to inspect travel, tools, holders,
fixtures, work offset and limits. Machine changes also require approval.
Do not use a convenient post merely to remove a refusal.

Panel prompt: “Regenerate the job and verify the whole program. Explain
each finding. If it passes, propose writing bracket.nc with the post I
selected. Wait for my confirmation.”

```json
{"jsonrpc":"2.0","id":40,"method":"tools/call","params":{"name":"generate","arguments":{}}}
```

```json
{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"verify","arguments":{}}}
```

Check the whole-program **ok** result, **gate** problems and **findings**.
Resolve all blocking findings first. The next example uses Fanuc only
when it matches the chosen machine and controller; otherwise replace it
with your correct built-in post name or .cps path.

```json
{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"post","arguments":{"output":"/absolute/output/bracket.nc","post":"Fanuc"}}}
```

The panel shows **Confirmation required**, the tool and its arguments.
Inspect the destination and post. **Allow** permits this request;
**Deny** refuses it. Posting always requires approval in the live app.
Approval does not bypass the post gate. A stale operation, gouge, collision
or exceeded machine limit still refuses the output.

Check the returned file paths. Inspect the NC program in the G-code viewer
and prove it on the machine with the normal shop procedure before cutting.
See [Posting](@/guide/check-and-post/posting.md).

## Privacy and troubleshooting

Cloud requests send the prompt, tool descriptions and conversation/tool
results to the selected provider. A model can read document details through
its tools. Only use a cloud provider for data you may send to it.
**Attach selection** is opt-in. **Attach screenshot** is disabled because
viewport capture is unavailable. Offline-only mode controls the in-app
transport, not other programs or their network activity.

| Symptom | What to check |
|---|---|
| A model cannot call tools | Ollama native mode refuses models without a tools capability. Select Local Prompt-JSON or Local Q&A. Q&A cannot edit the document. |
| A small context window loses earlier details | Work in short turns with one task. Ask for the current job again and inspect the tool replies. Do not treat an incomplete answer as a completed edit. |
| A connection refused error or “could not be reached” | Start the local runtime or the app listener first. Check the exact host and port. The panel's local presets need Ollama at port 11434. The MCP bridge uses the app's separate port, 7777 in this guide. |
| HTTP 401 or 403, or a rejected key | Check the named environment variable or keychain account, provider access and model ID. A stale environment key overrides the keychain. Restart after changing the launch environment. Do not paste the key into the conversation. |
| “offline-only mode blocks cloud provider” | Choose a local preset, or clear Offline only when cloud use is intended. |
| A tool is refused | Read its named reason and inspect the current document. A failed tool call is not a successful edit. |
| A live agent cannot open a project | Open it yourself with File > Open. Live sessions refuse replacing the document through `open_project`. |

Compact read modes and context budgets remain coming work in #1944.
Single-source agent skills remain coming work in #1947.
This guide describes the controls, flags and tools that have shipped.
