//! The silver assembler's refusals and drops, over synthetic parts labelled as the pipeline
//! labels them. Each refusal has a test of its own: one fault is made in a fresh labelling and the
//! preflight (`silver build --check-part`) or the build must refuse it, saying what is wrong.
//! Nothing here opens a holdout, owner or dev gold, and nothing needs a model, a key or the network.

use std::fs;

mod common;
use common::silver_parts::{Made, rename_ids, sha256};

/// The first sentence of the first part.
const FIRST: &str = "s0001";

/// One part, with `change` made to it, must be refused by the preflight, which says `needles`.
fn refused(change: impl FnOnce(&Made), needles: &[&str]) {
    let made = Made::with(1);
    change(&made);
    made.check_part(1).refused(needles);
}

/// The sentence ids of a part that the labels keep, in the draw's order.
fn kept(made: &Made, number: usize) -> Vec<String> {
    made.read(number, "merge/labelled.conllu")
        .lines()
        .filter_map(|line| line.strip_prefix("# sent_id "))
        .map(|rest| rest.trim_start_matches("= ").to_string())
        .collect()
}

/// Sets `column` (by its name in the draw's manifest) of every row of the part's manifest.
fn set_manifest(made: &Made, id: &str, column: &str, value: &str) {
    made.edit(1, "manifest.tsv", |text| {
        let mut head: Vec<String> = Vec::new();
        text.lines()
            .map(|line| {
                if line.starts_with('#') {
                    return format!("{line}\n");
                }
                let mut cells: Vec<String> = line.split('\t').map(str::to_string).collect();
                if head.is_empty() {
                    head = cells.clone();
                } else if cells[0] == id {
                    let at = head.iter().position(|name| name == column).unwrap();
                    cells[at] = value.to_string();
                }
                format!("{}\n", cells.join("\t"))
            })
            .collect()
    });
}

/// The path of the fixture the sentence `id` of part 1 quotes, and its sidecar's.
fn fixture(made: &Made, id: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let file = made.cell(1, id, "file");
    let md = made.tree.join(&file);
    let json = md.with_extension("json");
    (md, json)
}

/// Changes the sidecar of the fixture of `id` by `change` on its JSON.
fn edit_sidecar(made: &Made, id: &str, change: impl FnOnce(&mut serde_json::Value)) {
    let (_, json) = fixture(made, id);
    let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    change(&mut value);
    fs::write(&json, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

#[test]
fn a_part_that_was_labelled_passes_its_preflight_and_the_preflight_writes_nothing() {
    let made = Made::new();
    let before = common::draws::tree_bytes(&made.root);
    let said = made.check_part(1).ok();
    assert!(said.out.contains("part 01 of 2"), "{}", said.out);
    assert!(said.out.contains("5 sentences drawn, 5 labelled and kept, 0 dropped"), "{}", said.out);
    let said = made.check_part(2).ok();
    assert!(said.out.contains("part 02 of 2"), "{}", said.out);
    assert!(
        said.out.contains("4 sentences drawn, 3 labelled and kept, 0 dropped, 1 left out unsettled (1 words)"),
        "{}",
        said.out
    );
    assert_eq!(before, common::draws::tree_bytes(&made.root), "the preflight wrote something");
}

#[test]
fn labels_that_do_not_say_trains_yes_are_refused() {
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replace("# exam.trains = yes", "# exam.trains = no")),
        &["exam.trains = no", "finish --trains yes"],
    );
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replace("# exam.trains = yes\n", "")),
        &["exam.trains = nothing"],
    );
}

#[test]
fn an_id_of_dev_or_the_owner_is_refused() {
    refused(
        |made| {
            let dev = made.gold.join("dev.conllu");
            let mut text = fs::read_to_string(&dev).unwrap();
            text.push_str("# sent_id = s0001\n# text = Qqqx zzzx\n1\tQqqx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n\n");
            fs::write(dev, text).unwrap();
        },
        &["s0001", "its id is one of dev.conllu or owner.conllu"],
    );
}

#[test]
fn an_id_with_the_prefix_of_the_gold_flow_is_refused() {
    refused(
        |made| {
            for name in [
                "sample.conllu",
                "manifest.tsv",
                "merge/labelled.conllu",
                "merge/worklist.tsv",
                "merge/adjudicated.tsv",
                "merge/agreed.conllu",
            ] {
                made.edit(1, name, |text| rename_ids(text, "s", "g"));
            }
        },
        &["g0001", "an id of the gold flow"],
    );
}

#[test]
fn an_id_used_twice_is_refused() {
    refused(
        |made| {
            made.edit(1, "merge/labelled.conllu", |text| {
                // The second sentence takes the first one's id.
                text.replacen("# sent_id = s0003", "# sent_id = s0001", 1)
            });
        },
        &["s0001", "the id is used twice"],
    );
}

#[test]
fn a_draw_that_was_not_dealt_into_parts_is_refused() {
    refused(
        |made| made.edit(1, "manifest.tsv", |text| text.replace("# part = 1 of 1\n", "")),
        &["no `# part = k of N`", "no exemption for an undealt draw"],
    );
}

#[test]
fn a_draw_made_under_another_tag_version_is_refused() {
    refused(
        |made| made.edit(1, "manifest.tsv", |text| text.replace("# tag_version = 11\n", "# tag_version = 10\n")),
        &["tag version 10", "draw again"],
    );
    refused(
        |made| made.edit(1, "manifest.tsv", |text| text.replace("# tag_version = 11\n", "")),
        &["records no tag_version"],
    );
}

#[test]
fn tokens_the_exam_would_not_write_are_refused() {
    refused(
        |made| {
            made.edit(1, "sample.conllu", |text| {
                text.replacen("Kind=Word", "Kind=Word|Origin=Command", 1)
            })
        },
        &["not what `deslag-exam tokens` writes"],
    );
}

#[test]
fn labels_for_other_tokens_than_the_draws_are_refused() {
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replacen("\tproperty\t", "\tproperties\t", 1)),
        &["is not the token the draw holds"],
    );
}

#[test]
fn a_word_with_no_provenance_or_no_run_is_refused() {
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replacen("Prov=agree|", "", 1)),
        &["has no `Prov=`"],
    );
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replacen("Prov=agree", "Prov=blind", 1)),
        &["a silver word is agreed or adjudicated"],
    );
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replacen("|Runs=r1,r2,r3,r4", "", 1)),
        &["has no `Runs=`"],
    );
}

#[test]
fn a_word_with_no_part_of_speech_is_refused() {
    refused(
        |made| made.edit(1, "merge/labelled.conllu", |text| text.replacen("\tNOUN\t_\tNumber=Sing", "\t_\t_\t_", 1)),
        &["has no part of speech"],
    );
}

#[test]
fn a_sentence_that_is_neither_labelled_nor_unsettled_is_refused() {
    refused(
        |made| {
            // The labels lose a sentence and the unsettled list does not hold it.
            made.edit(1, "merge/labelled.conllu", |text| {
                let at = text.find("# sent_id = s0005").unwrap();
                let end = text[at..].find("\n\n").map_or(text.len(), |n| at + n + 2);
                format!("{}{}", &text[..at], &text[end..])
            });
        },
        &["s0005", "neither labelled nor listed as unsettled"],
    );
}

#[test]
fn a_sentence_both_labelled_and_listed_as_unsettled_is_refused() {
    refused(
        |made| {
            made.write(
                1,
                "merge/unsettled.tsv",
                "sent_id\ttoken\tform\ns0001\t1\tRenamed\n",
            );
        },
        &["s0001", "it is listed as unsettled and the labels hold it"],
    );
}

#[test]
fn a_merge_that_lacks_a_file_is_refused() {
    refused(
        |made| fs::remove_file(made.part(1).join("merge/agreement.txt")).unwrap(),
        &["agreement.txt", "run `finish --trains yes`"],
    );
    refused(
        |made| fs::remove_file(made.part(1).join("merge/adjudicator.json")).unwrap(),
        &["adjudicator.json", "the merge has no such file"],
    );
}

#[test]
fn an_adjudicator_json_that_names_another_adjudicator_is_refused() {
    refused(
        |made| made.write(1, "merge/adjudicator.json", "{\"name\": \"claude\", \"model\": \"anthropic/claude-sonnet-5.5\"}\n"),
        &["adjudicator.json names `claude`", "voters.json has `opus`"],
    );
}

#[test]
fn a_run_the_run_table_does_not_describe_is_refused() {
    refused(
        |made| {
            made.edit(1, "runs.tsv", |text| {
                text.lines().filter(|line| !line.starts_with("r2\t")).map(|l| format!("{l}\n")).collect()
            });
        },
        &["run r2 is named and runs.tsv does not describe it"],
    );
}

#[test]
fn a_run_that_is_not_complete_is_refused() {
    refused(
        |made| made.set_run(1, "r1", "status", "failed"),
        &["run r1 is `failed`", "only a complete run may vouch for a word"],
    );
}

#[test]
fn a_run_with_no_state_is_refused() {
    refused(|made| made.set_run(1, "r1", "state_id", "-"), &["run r1 has no state_id"]);
}

#[test]
fn a_run_id_under_two_states_is_refused() {
    refused(
        |made| {
            made.edit(1, "runs.tsv", |text| {
                let row = text.lines().find(|line| line.starts_with("r1\t")).unwrap();
                let other = row.replacen("\tst1\t", "\tst2\t", 1);
                format!("{text}{other}\n")
            });
        },
        &["run r1 is under 2 states", "a word names a run by its id alone"],
    );
}

#[test]
fn runs_at_more_than_one_commit_are_refused() {
    refused(
        |made| made.set_run(1, "r2", "deslag_commit", "fedcba9876543210fedcba9876543210fedcba98"),
        &["made at 2 commits of deslag"],
    );
}

#[test]
fn a_run_at_a_dirty_commit_is_refused() {
    let dirty = format!("{}-dirty", common::silver_parts::COMMIT);
    refused(
        |made| {
            for run in ["r1", "r2", "r3", "r4", "r5"] {
                made.set_run(1, run, "deslag_commit", &dirty);
            }
        },
        &["-dirty", "a run is at a clean commit of deslag"],
    );
}

#[test]
fn a_voter_at_an_endpoint_voters_json_does_not_list_is_refused() {
    refused(
        |made| made.set_run(1, "r1", "endpoint", "elsewhere/fp8"),
        &["run r1 was at endpoint elsewhere/fp8", "does not list for `deepseek`"],
    );
}

#[test]
fn a_voter_at_another_quantisation_is_refused() {
    refused(
        |made| made.set_run(1, "r2", "quantization", "int4"),
        &["run r2 was at quantization int4"],
    );
}

#[test]
fn a_voter_run_of_another_model_than_voters_json_pins_is_refused() {
    refused(
        |made| made.set_run(1, "r3", "model", "google/gemma-3-27b-it"),
        &["run r3 is of model google/gemma-3-27b-it"],
    );
}

#[test]
fn a_licence_other_than_mit_or_apache_is_refused() {
    refused(
        |made| made.set_run(1, "r1", "license", "GPL-3.0"),
        &["run r1 records the licence `GPL-3.0`", "MIT or Apache-2.0"],
    );
    refused(
        |made| made.set_run(1, "r4", "license", "Llama-3.1"),
        &["run r4 records the licence `Llama-3.1`"],
    );
}

#[test]
fn a_run_with_no_licence_or_no_date_for_it_is_refused() {
    refused(
        |made| made.set_run(1, "r2", "license", "-"),
        &["run r2 records no licence (`-`)"],
    );
    refused(
        |made| made.set_run(1, "r4", "license", "-"),
        &["run r4 records no licence (`-`)"],
    );
    refused(
        |made| made.set_run(1, "r2", "license_checked", "-"),
        &["run r2 records `-` as the date its licence was read"],
    );
}

#[test]
fn a_run_made_under_another_voters_json_is_refused() {
    refused(
        |made| made.set_run(1, "r1", "voters_sha256", &"f".repeat(64)),
        &["run r1 began under a voters.json of sha256"],
    );
    // A checkout whose voters.json changed since the runs cannot assemble them either.
    let made = Made::with(1);
    let mut text = fs::read_to_string(&made.voters).unwrap();
    text = text.replacen("\"batch_size\": 50", "\"batch_size\": 40", 1);
    fs::write(&made.voters, text).unwrap();
    made.check_part(1).refused(&["began under a voters.json of sha256"]);
}

#[test]
fn a_voter_that_voters_json_does_not_list_is_refused() {
    refused(
        |made| {
            made.set_run(1, "r1", "name", "mistral");
            made.set_run(1, "r1", "model", "mistralai/mistral-large-2512");
        },
        &["run r1 is of `mistral`, which voters.json does not list as a voter"],
    );
}

#[test]
fn an_adjudicator_other_than_the_confined_one_is_refused() {
    refused(
        |made| {
            made.set_run(1, "r5", "name", "claude");
            made.set_run(1, "r5", "model", "anthropic/claude-sonnet-5.5");
        },
        &["run r5 is of adjudicator `claude`", "the confined `opus`"],
    );
}

#[test]
fn an_adjudicator_run_with_no_agent_record_is_refused() {
    refused(
        |made| made.set_run(1, "r5", "settings", "-"),
        &["run r5 records no agent in its settings"],
    );
}

#[test]
fn an_agent_record_that_is_not_the_confined_process_is_refused() {
    let with = |change: fn(&mut serde_json::Value)| {
        let mut agent = common::silver_parts::agent();
        change(&mut agent);
        serde_json::json!({ "agent": agent }).to_string()
    };
    refused(
        |made| made.set_run(1, "r5", "settings", &with(|a| a["safe_mode"] = false.into())),
        &["does not say safe_mode true"],
    );
    refused(
        |made| made.set_run(1, "r5", "settings", &with(|a| a["tools"] = "Read,Write,Bash".into())),
        &["has other tools than Read,Write"],
    );
    refused(
        |made| made.set_run(1, "r5", "settings", &with(|a| a["version"] = "".into())),
        &["its agent record lacks version"],
    );
    refused(
        |made| made.set_run(1, "r5", "settings", &with(|a| a["prompt_sha256"] = "short".into())),
        &["no prompt sha256"],
    );
}

#[test]
fn an_outside_tagger_other_than_spacy_is_refused() {
    refused(
        |made| made.set_run(1, "r4", "name", "harper"),
        &["only spacy may vote (D3)"],
    );
    refused(
        |made| made.set_run(1, "r4", "model", "en-core-web-sm"),
        &["run r4 is of model en-core-web-sm", "does not give for `spacy`"],
    );
}

#[test]
fn a_merge_with_fewer_than_three_model_voters_is_refused() {
    refused(
        |made| {
            made.edit(1, "merge/voters.tsv", |text| {
                text.lines()
                    .filter(|line| !line.starts_with("C\t"))
                    .map(|line| format!("{line}\n"))
                    .collect()
            });
        },
        &["the merge has 2 model voters", "at least 3"],
    );
}

#[test]
fn a_merge_that_agreed_words_on_fewer_than_three_voters_is_refused() {
    refused(
        |made| made.edit(1, "merge/voters.tsv", |text| text.replace("# min_voters = 3", "# min_voters = 2")),
        &["min_voters is 2", "at least 3 model voters"],
    );
}

#[test]
fn a_merge_with_no_spacy_is_refused() {
    refused(
        |made| {
            made.edit(1, "merge/voters.tsv", |text| {
                text.lines()
                    .filter(|line| !line.starts_with("D\t"))
                    .map(|line| format!("{line}\n"))
                    .collect()
            });
        },
        &["the merge has no spacy among its voters"],
    );
}

#[test]
fn a_voter_with_no_run_or_the_run_of_another_voter_is_refused() {
    refused(
        |made| made.edit(1, "merge/voters.tsv", |text| text.replace("\tr2\t", "\t-\t")),
        &["voter qwen has no run recorded"],
    );
    refused(
        |made| made.edit(1, "merge/voters.tsv", |text| text.replace("\tr2\t", "\tr1\t")),
        &["voter qwen names run r1", "a run of `deepseek`"],
    );
}

#[test]
fn a_voter_that_voters_json_does_not_name_is_refused_in_the_merge() {
    refused(
        |made| made.edit(1, "merge/voters.tsv", |text| text.replace("\tgemma\t", "\tmistral\t")),
        &["voter mistral is not a voter of voters.json"],
    );
}

#[test]
fn a_fixture_the_image_lacks_is_refused() {
    refused(
        |made| {
            let (md, json) = fixture(made, "s0003");
            fs::remove_file(md).unwrap();
            fs::remove_file(json).unwrap();
        },
        &["the pinned image does not hold it"],
    );
}

#[test]
fn a_fixture_whose_content_commit_or_url_changed_is_refused() {
    refused(
        |made| set_manifest(made, "s0003", "content_sha256", &"0".repeat(64)),
        &["its content is not what the draw quoted (sha256 differs)"],
    );
    refused(
        |made| set_manifest(made, "s0003", "source_commit", &"0".repeat(40)),
        &["its commit is not the one the draw quoted"],
    );
    refused(
        |made| set_manifest(made, "s0003", "source_url", "https://example.org/x"),
        &["its URL is not the one the draw quoted"],
    );
}

#[test]
fn a_fixture_whose_licence_differs_from_the_manifests_is_refused() {
    refused(
        |made| {
            let other = if made.cell(1, "s0003", "license") == "MIT" { "BSD-3-Clause" } else { "MIT" };
            set_manifest(made, "s0003", "license", other);
        },
        &["sidecar licence", "is not the manifest's"],
    );
}

#[test]
fn a_fixture_whose_licence_is_outside_the_accepted_list_is_refused() {
    refused(
        |made| {
            edit_sidecar(made, "s0003", |value| value["source"]["license"] = "GPL-3.0-only".into());
            set_manifest(made, "s0003", "license", "GPL-3.0-only");
        },
        &["a fixture does not load"],
    );
}

#[test]
fn a_fixture_the_exclusion_list_names_is_refused() {
    refused(
        |made| {
            let sha = sha256(&fs::read(fixture(made, "s0003").0).unwrap());
            fs::write(&made.exclude, format!("{}\n{sha}\n", "0".repeat(64))).unwrap();
        },
        &["the exclusion list names it"],
    );
}

#[test]
fn a_fixture_of_another_repository_than_the_manifests_is_refused() {
    refused(
        |made| set_manifest(made, "s0003", "repo", "elsewhere/else"),
        &["its repository is not the one the manifest names"],
    );
}

#[test]
fn a_fixture_whose_declared_generator_is_banned_is_refused() {
    refused(
        |made| {
            // An llm fixture of the part, made a publisher-declared one whose model is a Llama.
            let id = made
                .ids(1)
                .into_iter()
                .find(|id| made.cell(1, id, "tier") == "llm")
                .expect("the part holds an llm sentence");
            let mut repo = String::new();
            edit_sidecar(made, &id, |value| {
                let source = &mut value["source"];
                repo = format!("datasets/owner/{}", id);
                source["repo"] = repo.clone().into();
                let path = source["path"].as_str().unwrap().to_string();
                value["sidecar_version"] = 4.into();
                let object = value.as_object_mut().unwrap();
                object.remove("history");
                object.remove("before");
                value["declared"] = serde_json::json!({
                    "dataset": format!("owner/{id}"),
                    "revision": value["source"]["commit"],
                    "file": path,
                    "file_sha256": "0".repeat(64),
                    "row": 1,
                    "row_id": "x",
                    "model": "Llama-3-70B",
                    "model_license": "MIT",
                    "model_license_card": "https://example.org/card",
                    "statement": "the model that wrote it",
                    "columns": {},
                });
            });
            set_manifest(made, &id, "repo", &repo);
            set_manifest(made, &id, "model", "Llama-3-70B");
            set_manifest(made, &id, "model_license", "MIT");
        },
        &["its declared generator is of the banned family `llama`"],
    );
}

#[test]
fn a_path_of_the_makers_machine_in_a_file_the_part_ships_is_refused() {
    // A listing.
    refused(
        |made| made.write(1, "listings/r1.json", "{\"note\": \"/home/someone/.label/silver\"}\n"),
        &["listings/r1.json", "/home/someone/.label/silver", "a path of the machine that made it"],
    );
    // A cell of the run table, inside the JSON of `settings`.
    refused(
        |made| {
            let agent = {
                let mut agent = common::silver_parts::agent();
                agent["cwd"] = "/tmp/scratch/empty".into();
                agent
            };
            made.set_run(1, "r5", "settings", &serde_json::json!({ "agent": agent }).to_string());
        },
        &["/tmp/scratch/empty", "a path of the machine that made it"],
    );
    // A cell of the adjudicator's record.
    refused(
        |made| made.write(1, "merge/adjudicator.json", "{\"name\": \"opus\", \"model\": \"claude-opus-5-5\", \"dir\": \"~/x\"}\n"),
        &["adjudicator.json", "~/x"],
    );
}

#[test]
fn the_voters_file_of_a_merge_may_hold_paths_because_a_batch_ships_it_without_them() {
    let made = Made::with(1);
    let voters = made.read(1, "merge/voters.tsv");
    assert!(voters.contains("/tags/deepseek.conllu"), "{voters}");
    assert!(made.root.is_absolute());
    made.check_part(1).ok();
}

#[test]
fn a_listing_a_used_run_needs_is_refused_when_it_is_missing() {
    refused(
        |made| fs::remove_file(made.part(1).join("listings/r1.json")).unwrap(),
        &["listings/r1.json"],
    );
}

#[test]
fn a_part_in_the_image_that_is_not_a_labelling_draw_is_refused() {
    refused(
        |made| {
            made.edit(1, "manifest.tsv", |text| {
                text.replace("# draw = for labelling, split unlabelled", "# draw = for gold, split unlabelled")
            })
        },
        &["under `.label` only a skeleton", "a labelling draw"],
    );
}

#[test]
fn a_sentence_whose_text_became_gold_is_dropped_and_counted_not_refused() {
    let made = Made::with(1);
    // The gold now holds the text of a sentence the labels kept.
    let labelled = made.read(1, "merge/labelled.conllu");
    let block: String = labelled
        .split("\n\n")
        .find(|block| block.contains("# sent_id = s0004"))
        .unwrap()
        .lines()
        .filter(|line| {
            line.starts_with("# text") || !line.starts_with('#')
        })
        .map(|line| {
            if line.starts_with('#') {
                return line.to_string();
            }
            let mut cells: Vec<String> = line.split('\t').map(str::to_string).collect();
            cells[9] = cells[9]
                .split('|')
                .filter(|entry| !entry.starts_with("Runs="))
                .collect::<Vec<_>>()
                .join("|");
            cells.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dev = made.gold.join("dev.conllu");
    let mut text = fs::read_to_string(&dev).unwrap();
    text.push_str(&format!("# sent_id = g0900\n{block}\n\n"));
    fs::write(&dev, text).unwrap();
    let said = made.check_part(1).ok();
    assert!(said.out.contains("7 labelled and kept, 1 dropped"), "{}", said.out);
    assert!(said.out.contains("dropped, gold text: 1"), "{}", said.out);
    let built = made.build("2026-10-08-gold", &[]).ok();
    assert!(built.out.contains("dropped 1"), "{}", built.out);
    let out = made.out("2026-10-08-gold");
    let drops = fs::read_to_string(out.join("record/drops.tsv")).unwrap();
    assert!(drops.contains("s0004\t01\tgold text\n"), "{drops}");
    let silver = fs::read_to_string(out.join("silver.conllu")).unwrap();
    assert!(!silver.contains("# sent_id = s0004"));
    let manifest = fs::read_to_string(out.join("manifest.tsv")).unwrap();
    assert!(!manifest.contains("\ns0004\t"));
    let sheet = fs::read_to_string(out.join("DATASHEET.md")).unwrap();
    assert!(sheet.contains("gold text"), "{sheet}");
}

#[test]
fn a_sentence_whose_repository_became_reserved_is_dropped_and_counted_not_refused() {
    let made = Made::with(1);
    let repo = made.cell(1, "s0001", "repo");
    let same: Vec<String> = made
        .ids(1)
        .into_iter()
        .filter(|id| made.cell(1, id, "repo") == repo && kept(&made, 1).contains(id))
        .collect();
    assert!(!same.is_empty());
    // Dev now names the repository.
    let manifest = made.gold.join("dev.manifest.tsv");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str(&format!("g2\tdev\thuman\tprose\tx.md\t{repo}\tMIT\t0-1\n"));
    fs::write(manifest, text).unwrap();
    let said = made.check_part(1).ok();
    assert!(said.out.contains(&format!("{} dropped", same.len())), "{}", said.out);
    assert!(said.out.contains(&format!("dropped, reserved repository: {}", same.len())), "{}", said.out);
    made.build("2026-10-08-reserved", &[]).ok();
    let drops = fs::read_to_string(made.out("2026-10-08-reserved").join("record/drops.tsv")).unwrap();
    for id in same {
        assert!(drops.contains(&format!("{id}\t01\treserved repository\n")), "{drops}");
    }
}

#[test]
fn a_sentence_the_small_tier_now_holds_is_dropped_as_a_reserved_repository() {
    let made = Made::with(1);
    let repo = made.cell(1, FIRST, "repo");
    // The small tier gains a fixture of the repository.
    let (md, json) = fixture(&made, FIRST);
    let tier = made.cell(1, FIRST, "tier");
    let dir = made.small.join(tier).join("added");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(&md, dir.join(md.file_name().unwrap())).unwrap();
    fs::copy(&json, dir.join(json.file_name().unwrap())).unwrap();
    let said = made.check_part(1).ok();
    assert!(said.out.contains("dropped, reserved repository"), "{}", said.out);
    assert!(repo.contains('/'));
}

/// Two parts of a draw, as the labelling hands them to the assembler.
fn two_parts() -> Made {
    Made::new()
}

#[test]
fn parts_of_two_different_draws_are_refused() {
    let made = two_parts();
    made.edit(2, "manifest.tsv", |text| text.replacen("# seed = 0x6465736c6167", "# seed = 0x1", 1));
    made.build("2026-10-08-draws", &[]).refused(&["part 02 is of another draw than part 01"]);
    assert!(!made.out("2026-10-08-draws").exists());
}

#[test]
fn the_same_part_twice_is_refused() {
    let made = two_parts();
    made.build_parts("2026-10-08-twice", &[made.spec(1), made.spec(1)], &[])
        .refused(&["two --part arguments are the same part of the draw"]);
}

#[test]
fn parts_with_other_voters_are_refused() {
    let made = two_parts();
    made.edit(2, "merge/voters.tsv", |text| text.replace("# min_voters = 3", "# min_voters = 4"));
    made.build("2026-10-08-voters", &[]).refused(&["part 02 has other voters than part 01"]);
}

#[test]
fn a_run_id_written_twice_with_different_rows_across_parts_is_refused() {
    let made = two_parts();
    // Part 2's table also describes part 1's first run, with another number of calls.
    made.edit(2, "runs.tsv", |text| {
        let row = made.read(1, "runs.tsv").lines().find(|line| line.starts_with("r1\t")).unwrap().to_string();
        let mut cells: Vec<&str> = row.split('\t').collect();
        cells[15] = "99";
        format!("{text}{}\n", cells.join("\t"))
    });
    made.build("2026-10-08-rows", &[]).refused(&["run r1 is written twice under state st1 with different rows"]);
}

#[test]
fn two_prompts_for_one_model_across_parts_are_refused() {
    let made = two_parts();
    made.set_run(2, "r6", "prompt_sha256", &"6".repeat(64));
    made.build("2026-10-08-prompts", &[]).refused(&["model deepseek/deepseek-v4-flash has 2 prompt_sha256 values"]);
}

#[test]
fn two_guides_for_one_model_across_parts_are_refused() {
    let made = two_parts();
    made.set_run(2, "r7", "guide_sha256", &"5".repeat(64));
    made.build("2026-10-08-guides", &[]).refused(&["has 2 guide_sha256 values"]);
}

#[test]
fn runs_at_two_commits_across_parts_are_refused() {
    let made = two_parts();
    for run in ["r6", "r7", "r8", "r9", "r10"] {
        made.set_run(2, run, "deslag_commit", "fedcba9876543210fedcba9876543210fedcba98");
    }
    made.build("2026-10-08-commits", &[]).refused(&["made at 2 commits of deslag"]);
}

#[test]
fn two_agents_across_parts_are_refused() {
    let made = two_parts();
    let mut agent = common::silver_parts::agent();
    agent["version"] = "2.1.294".into();
    made.set_run(2, "r10", "settings", &serde_json::json!({ "agent": agent }).to_string());
    made.build("2026-10-08-agents", &[]).refused(&["record 2 different agents"]);
}

#[test]
fn a_repeated_text_is_dropped_as_a_repeat() {
    // Part 2 is a copy of part 1 under other ids: every one of its sentences repeats a text.
    let made = Made::with(1);
    made.edit(1, "manifest.tsv", |text| text.replace("# part = 1 of 1", "# part = 1 of 2"));
    let copy = made.root.join(".label/silver/part-02");
    copy_dir(made.part(1), &copy);
    for name in [
        "sample.conllu",
        "manifest.tsv",
        "merge/labelled.conllu",
        "merge/worklist.tsv",
        "merge/adjudicated.tsv",
        "merge/agreed.conllu",
        "merge/unsettled.tsv",
    ] {
        let path = copy.join(name);
        let text = fs::read_to_string(&path).unwrap();
        let renamed = rename_ids(&text, "s", "t")
            .replace("# part = 1 of 2", "# part = 2 of 2")
            .replace("ids t0001 on", "ids s0001 on");
        fs::write(&path, renamed).unwrap();
    }
    let spec = |dir: &std::path::Path| format!("{}:merge", dir.display());
    made.build_parts("2026-10-08-repeat", &[spec(made.part(1)), spec(&copy)], &[]).ok();
    let out = made.out("2026-10-08-repeat");
    let drops = fs::read_to_string(out.join("record/drops.tsv")).unwrap();
    let repeats = drops.lines().filter(|line| line.ends_with("\trepeat")).count();
    assert_eq!(repeats, 8, "{drops}");
    assert!(drops.contains("t0001\t02\trepeat\n"), "{drops}");
}

/// Copies the directory `from` to `to`.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn an_out_directory_that_is_not_named_for_the_batch_is_refused() {
    let made = two_parts();
    let wrong = made.root.join("out").join("another-name");
    let mut args: Vec<String> = [
        "silver",
        "build",
        "--name",
        "2026-10-08-x",
        "--out",
        wrong.to_str().unwrap(),
        "--part",
        &made.spec(1),
        "--annotations-license",
        "CC-BY-4.0",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    args.extend(made.pool_args());
    args.extend(made.corpus_args());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    made.gold(&args).refused(&["--out must be a directory named `2026-10-08-x`"]);
    assert!(!wrong.exists());
}

#[test]
fn an_audit_without_the_archive_hash_is_refused() {
    let made = two_parts();
    let audit = made.root.join("audit");
    fs::create_dir_all(&audit).unwrap();
    let ran = made.build("2026-10-08-audit", &["--audit", audit.to_str().unwrap()]);
    ran.refused(&["--archive-sha256"]);
}
