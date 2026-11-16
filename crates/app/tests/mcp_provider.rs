//! Provider schemas, calls, and local fallbacks for the live MCP surface.
//! Provenance: public provider API formats and the issue #1943 contract.
//! Gate map: covers app::mcp::provider, app::mcp::provider_calls,
//! app::mcp::fallback, app::mcp::schema_validation, app::mcp::tools.

use camcraft_app::mcp::Server;
use camcraft_app::mcp::provider::{
    FallbackMode, Provider, parse_calls, parse_prompt, prompt_context, qa_context, tool_result, tools,
};
use serde_json::{Value, json};

fn live_tools() -> Vec<Value> {
    let reply = Server::new()
        .handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .expect("tools/list response");
    reply["result"]["tools"].as_array().expect("live tools").clone()
}

fn gemini_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if key == "$schema" {
                    continue;
                }
                if key == "type"
                    && let Some(types) = value.as_array()
                {
                    out.insert(
                        "anyOf".into(),
                        json!(types.iter().map(|kind| json!({"type":kind})).collect::<Vec<_>>()),
                    );
                } else {
                    out.insert(key.clone(), gemini_schema(value));
                }
            }
            Value::Object(out)
        }
        Value::Array(rows) => Value::Array(rows.iter().map(gemini_schema).collect()),
        value => value.clone(),
    }
}

#[test]
fn every_live_tool_maps_without_schema_loss() {
    let live = live_tools();
    assert!(!live.is_empty());
    for provider in [
        Provider::Anthropic,
        Provider::OpenAiChat,
        Provider::OpenAiResponses,
        Provider::Gemini,
    ] {
        let mapped = tools(provider);
        let rows = if provider == Provider::Gemini {
            mapped[0]["functionDeclarations"]
                .as_array()
                .expect("Gemini declarations")
        } else {
            &mapped
        };
        assert_eq!(rows.len(), live.len(), "{provider:?}");
        for (source, mapped) in live.iter().zip(rows) {
            let row = if provider == Provider::OpenAiChat {
                &mapped["function"]
            } else {
                mapped
            };
            assert_eq!(row["name"], source["name"], "{provider:?}");
            assert_eq!(row["description"], source["description"], "{provider:?}");
            match provider {
                Provider::Anthropic => assert_eq!(row["input_schema"], source["inputSchema"]),
                Provider::Gemini => assert_eq!(row["parametersJsonSchema"], gemini_schema(&source["inputSchema"])),
                Provider::OpenAiChat | Provider::OpenAiResponses => {
                    assert_eq!(mapped["type"], "function");
                    assert_eq!(row["strict"], false);
                    assert_eq!(row["parameters"], source["inputSchema"]);
                }
            }
        }
    }
}

#[test]
fn provider_calls_and_results_keep_the_call_identity() {
    let cases = [
        (
            Provider::Anthropic,
            json!({"content":[{"type":"text","text":"Reading"},{"type":"tool_use","id":"call-7","name":"get_job","input":{}}]}),
        ),
        (
            Provider::OpenAiChat,
            json!({"choices":[{"message":{"tool_calls":[{"id":"call-7","type":"function","function":{"name":"get_job","arguments":"{}"}}]}}]}),
        ),
        (
            Provider::OpenAiResponses,
            json!({"output":[{"type":"function_call","id":"item-7","call_id":"call-7","name":"get_job","arguments":"{}"}]}),
        ),
        (
            Provider::Gemini,
            json!({"candidates":[{"content":{"parts":[{"functionCall":{"id":"call-7","name":"get_job","args":{}}}]}}]}),
        ),
    ];
    for (provider, response) in cases {
        let calls = parse_calls(provider, &response).expect("provider call");
        assert_eq!(calls.len(), 1, "{provider:?}");
        let call = &calls[0];
        assert_eq!(call.id, "call-7", "{provider:?}");
        assert_eq!(call.name, "get_job");
        assert_eq!(call.arguments, json!({}));
        let result = tool_result(
            provider,
            call,
            &json!({"content":[{"type":"text","text":"job data"}],"isError":true}),
        );
        match provider {
            Provider::Anthropic => {
                assert_eq!(result["type"], "tool_result");
                assert_eq!(result["tool_use_id"], "call-7");
                assert_eq!(result["is_error"], true);
            }
            Provider::OpenAiChat => {
                assert_eq!(result["role"], "tool");
                assert_eq!(result["tool_call_id"], "call-7");
                assert!(result["content"].as_str().expect("content").contains("job data"));
            }
            Provider::OpenAiResponses => {
                assert_eq!(result["type"], "function_call_output");
                assert_eq!(result["call_id"], "call-7");
                assert!(result["output"].as_str().expect("output").contains("job data"));
            }
            Provider::Gemini => {
                assert_eq!(result["functionResponse"]["name"], "get_job");
                assert_eq!(result["functionResponse"]["id"], "call-7");
                assert!(result["functionResponse"]["response"].is_object());
            }
        }
    }
}

#[test]
fn prompt_parser_accepts_one_object_and_refuses_untrusted_shapes() {
    for text in [
        r#"{"tool":"get_job","arguments":{}}"#,
        "```json\n{\"tool\":\"get_job\",\"arguments\":{}}\n```",
    ] {
        let pending = parse_prompt(text).expect("one JSON block");
        assert!(!pending.requires_confirmation());
        assert_eq!(pending.authorize(false).expect("read-only call").name, "get_job");
    }
    for text in [
        "",
        "[]",
        "null",
        "{}",
        r#"{"tool":"get_job","arguments":[]}"#,
        r#"{"tool":"get_job","arguments":{}} {"tool":"undo","arguments":{}}"#,
        r#"Here is a call: {"tool":"get_job","arguments":{}}"#,
        "```json\n{}\n```\n```json\n{}\n```",
        "```json\n{\"tool\":\"get_job\",\"arguments\":{}}",
        r#"{"tool":"get_job","arguments":{},"extra":"ignored?"}"#,
    ] {
        assert!(parse_prompt(text).is_err(), "accepted {text}");
    }
    let error = parse_prompt(r#"{"tool":"invented_tool","arguments":{}}"#)
        .expect_err("unknown tool refused");
    assert!(error.contains("invented_tool"), "{error}");
    assert!(error.contains("unknown tool"), "{error}");
}

#[test]
fn schema_validation_names_bad_arguments_before_confirmation() {
    let cases = [
        ("add_accessory", json!({}), "kind"),
        ("add_accessory", json!({"kind":"invented"}), "kind"),
        ("add_accessory", json!({"kind":"block","steps":0}), "steps"),
        ("add_accessory", json!({"kind":"block","steps":1.5}), "steps"),
        ("add_accessory", json!({"kind":"block","position":[0,0]}), "position"),
        (
            "add_accessory",
            json!({"kind":"block","position":[0,0,0,0]}),
            "position",
        ),
        (
            "add_accessory",
            json!({"kind":"block","dimensions":{"height":"bad"}}),
            "height",
        ),
        (
            "import_model",
            json!({"path":"part.step","facet_budget":1000000000000_u64}),
            "facet_budget",
        ),
        ("get_job", json!({"unexpected":1}), "unexpected"),
        ("set_stock", json!({"box":{"dx":1,"dy":2}}), "dz"),
        (
            "set_tool_cutter_profile",
            json!({"tool":"mill","points":[[0,"bad"]]}),
            "points",
        ),
        (
            "set_tool_cutter_profile",
            json!({"tool":"mill","points":true}),
            "points",
        ),
    ];
    for (tool, args, field) in cases {
        let text = json!({"tool":tool,"arguments":args}).to_string();
        let error = parse_prompt(&text).expect_err("invalid arguments refused");
        assert!(error.contains(field), "{tool}.{field}: {error}");
    }
    for (tool, args) in [
        ("add_accessory", json!({"kind":"block","dimensions":{"height":3.0}})),
        ("set_tool_cutter_profile", json!({"tool":"mill","points":null})),
        (
            "set_tool_cutter_profile",
            json!({"tool":"mill","points":[[0,0],[10,2,0]]}),
        ),
    ] {
        assert!(
            parse_prompt(&json!({"tool":tool,"arguments":args}).to_string()).is_ok(),
            "{tool}"
        );
    }
}

#[test]
fn mutating_fallback_calls_wait_for_explicit_confirmation() {
    for (tool, arguments) in [
        ("undo", json!({})),
        ("save_project", json!({})),
        ("set_stock", json!({"box":{"dx":10,"dy":10,"dz":10}})),
        ("detect_pockets", json!({"model":"part","create":0,"tool":"mill"})),
    ] {
        let text = json!({"tool":tool,"arguments":arguments}).to_string();
        assert!(
            parse_prompt(&text).expect("pending call").requires_confirmation(),
            "{tool}"
        );
        assert!(
            parse_prompt(&text).expect("pending call").authorize(false).is_err(),
            "{tool}"
        );
        assert_eq!(
            parse_prompt(&text)
                .expect("pending call")
                .authorize(true)
                .expect("confirmed call")
                .name,
            tool
        );
    }
}

#[test]
fn no_native_tools_selects_the_two_local_fallback_tiers() {
    assert_eq!(FallbackMode::for_capabilities(true, true), FallbackMode::Native);
    assert_eq!(FallbackMode::for_capabilities(true, false), FallbackMode::Native);
    assert_eq!(FallbackMode::for_capabilities(false, true), FallbackMode::PromptJson);
    assert_eq!(FallbackMode::for_capabilities(false, false), FallbackMode::ReadOnly);
    let project = json!({"setups":[{"name":"Setup1"}],"operations":[],"note":"do not edit"});
    let original = project.clone();
    let context = qa_context(&project);
    assert_eq!(context["project"], original);
    assert_eq!(context["tools"], json!([]));
    assert_eq!(project, original);
    assert!(context.to_string().contains("read-only"));
    let prompt = prompt_context(&project);
    assert_eq!(prompt["project"], original);
    assert_eq!(prompt["tools"], json!(live_tools()));
    let instructions = prompt["instructions"].as_str().expect("JSON mode instructions");
    assert!(instructions.contains("tool"));
    assert!(instructions.contains("arguments"));
    assert!(instructions.contains("confirm"));
    assert!(instructions.contains("JSON"));
}

#[test]
fn native_calls_refuse_invalid_arguments_and_malformed_json() {
    for invalid in [Value::Null, json!({"error":{"message":"unavailable"}})] {
        assert!(parse_calls(Provider::Anthropic, &invalid).is_err());
    }
    let non_array = json!({"content":{"type":"tool_use"}});
    assert!(parse_calls(Provider::Anthropic, &non_array).is_err());
    let wrong_type = json!({"choices":[{"message":{"tool_calls":[{"type":"custom","id":"x","function":{"name":"get_job","arguments":"{}"}}]}}]});
    assert!(parse_calls(Provider::OpenAiChat, &wrong_type).is_err());
    let empty_name = json!({"content":[{"type":"tool_use","id":"x","name":"","input":{}}]});
    let error = parse_calls(Provider::Anthropic, &empty_name).expect_err("empty name");
    assert!(error.contains("provider name"), "{error}");
    let missing_input = json!({"content":[{"type":"tool_use","id":"x","name":"get_job"}]});
    assert!(parse_calls(Provider::Anthropic, &missing_input).is_err());
    let invalid =
        json!({"content":[{"type":"tool_use","id":"x","name":"add_accessory","input":{"kind":"block","steps":0}}]});
    let error = parse_calls(Provider::Anthropic, &invalid)
        .expect_err("invalid tool input");
    assert!(error.contains("steps"), "{error}");
    let malformed = json!({"choices":[{"message":{"tool_calls":[{"id":"x","type":"function","function":{"name":"get_job","arguments":"not JSON"}}]}}]});
    assert!(parse_calls(Provider::OpenAiChat, &malformed).is_err());
}

#[test]
fn text_responses_have_no_calls_and_parallel_calls_are_preserved() {
    for (provider, response) in [
        (Provider::Anthropic, json!({"content":[{"type":"text","text":"Ready"}]})),
        (
            Provider::OpenAiChat,
            json!({"choices":[{"message":{"content":"Ready"}}]}),
        ),
        (
            Provider::OpenAiResponses,
            json!({"output":[{"type":"message","content":[{"type":"output_text","text":"Ready"}]}]}),
        ),
        (
            Provider::Gemini,
            json!({"candidates":[{"content":{"parts":[{"text":"Ready"}]}}]}),
        ),
    ] {
        assert!(
            parse_calls(provider, &response).expect("text response").is_empty(),
            "{provider:?}"
        );
    }
    let parallel = json!({"content":[
        {"type":"tool_use","id":"a","name":"get_job","input":{}},
        {"type":"tool_use","id":"b","name":"list_operation_types","input":{}}
    ]});
    let calls = parse_calls(Provider::Anthropic, &parallel).expect("parallel calls");
    assert_eq!(
        calls.iter().map(|call| call.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(calls[1].name, "list_operation_types");
}

#[test]
fn native_call_ids_are_required_and_unique_except_optional_gemini_id() {
    for (provider, missing_id) in [
        (
            Provider::Anthropic,
            json!({"content":[{"type":"tool_use","name":"get_job","input":{}}]}),
        ),
        (
            Provider::OpenAiChat,
            json!({"choices":[{"message":{"tool_calls":[{"type":"function","function":{"name":"get_job","arguments":"{}"}}]}}]}),
        ),
        (
            Provider::OpenAiResponses,
            json!({"output":[{"type":"function_call","id":"item-only","name":"get_job","arguments":"{}"}]}),
        ),
    ] {
        assert!(parse_calls(provider, &missing_id).is_err(), "{provider:?}");
    }
    let duplicate = json!({"content":[
        {"type":"tool_use","id":"a","name":"get_job","input":{}},
        {"type":"tool_use","id":"a","name":"get_job","input":{}}
    ]});
    assert!(parse_calls(Provider::Anthropic, &duplicate).is_err());
    let response = json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_job","args":{}}}]}}]});
    let calls = parse_calls(Provider::Gemini, &response).expect("Gemini id is optional");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].id.is_empty());
    let result = tool_result(Provider::Gemini, &calls[0], &json!({"isError":false}));
    assert!(result["functionResponse"].get("id").is_none());
}
