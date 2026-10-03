//! The annotation guide's tag codes: `N.s`, `V.pp`, `PR`, `X`.
//!
//! A code is a base, `N` for a noun and so on, and a feature after a dot. A tagger writes codes
//! and this module turns them into CoNLL-U's UPOS and FEATS, the way the guide says a script
//! does. It also reads UPOS and FEATS back into a code, which is how Harper's and spaCy's answers
//! are compared with the blind tagger's.

use std::fmt;

use deslag_exam::conllu;

/// The guide's bases, one per UPOS but `C`, which stands for both kinds of conjunction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Base {
    /// `N`, a common noun.
    N,
    /// `PN`, a proper noun.
    Pn,
    /// `V`, a verb.
    V,
    /// `AX`, an auxiliary.
    Ax,
    /// `J`, an adjective.
    J,
    /// `R`, an adverb.
    R,
    /// `PR`, a pronoun.
    Pr,
    /// `D`, a determiner.
    D,
    /// `P`, an adposition.
    P,
    /// `C`, a conjunction.
    C,
    /// `T`, a particle.
    T,
    /// `NM`, a numeral.
    Nm,
    /// `I`, an interjection.
    I,
    /// `X`, other.
    X,
}

impl Base {
    /// Every base, in the order of the guide's table.
    pub const ALL: [Base; 14] = [
        Base::N,
        Base::Pn,
        Base::V,
        Base::Ax,
        Base::J,
        Base::R,
        Base::Pr,
        Base::D,
        Base::P,
        Base::C,
        Base::T,
        Base::Nm,
        Base::I,
        Base::X,
    ];

    /// What the guide writes.
    pub fn code(self) -> &'static str {
        match self {
            Base::N => "N",
            Base::Pn => "PN",
            Base::V => "V",
            Base::Ax => "AX",
            Base::J => "J",
            Base::R => "R",
            Base::Pr => "PR",
            Base::D => "D",
            Base::P => "P",
            Base::C => "C",
            Base::T => "T",
            Base::Nm => "NM",
            Base::I => "I",
            Base::X => "X",
        }
    }

    /// The base the guide writes `code` as.
    pub fn from_code(code: &str) -> Option<Base> {
        Base::ALL.into_iter().find(|base| base.code() == code)
    }

    /// Whether the guide marks number on it: nouns, proper nouns and pronouns.
    pub fn takes_number(self) -> bool {
        matches!(self, Base::N | Base::Pn | Base::Pr)
    }

    /// Whether the guide marks a verb form on it: verbs and auxiliaries.
    pub fn takes_form(self) -> bool {
        matches!(self, Base::V | Base::Ax)
    }

    /// The UPOS of `form` tagged this way. `C` is `CCONJ` for the coordinators the guide lists and
    /// `SCONJ` for the rest.
    pub fn upos(self, form: &str) -> &'static str {
        match self {
            Base::N => "NOUN",
            Base::Pn => "PROPN",
            Base::V => "VERB",
            Base::Ax => "AUX",
            Base::J => "ADJ",
            Base::R => "ADV",
            Base::Pr => "PRON",
            Base::D => "DET",
            Base::P => "ADP",
            Base::C => {
                const COORDINATORS: [&str; 9] = [
                    "and", "or", "but", "nor", "yet", "plus", "both", "either", "neither",
                ];
                if COORDINATORS.contains(&form.to_lowercase().as_str()) {
                    "CCONJ"
                } else {
                    "SCONJ"
                }
            }
            Base::T => "PART",
            Base::Nm => "NUM",
            Base::I => "INTJ",
            Base::X => "X",
        }
    }

    /// The base a UPOS stands for. `PUNCT` and `SYM` on a word are `X`, since the guide has no
    /// other code for a word that is not one of the thirteen.
    pub fn from_upos(upos: &str) -> Option<Base> {
        Some(match upos {
            "NOUN" => Base::N,
            "PROPN" => Base::Pn,
            "VERB" => Base::V,
            "AUX" => Base::Ax,
            "ADJ" => Base::J,
            "ADV" => Base::R,
            "PRON" => Base::Pr,
            "DET" => Base::D,
            "ADP" => Base::P,
            "CCONJ" | "SCONJ" => Base::C,
            "PART" => Base::T,
            "NUM" => Base::Nm,
            "INTJ" => Base::I,
            "X" | "PUNCT" | "SYM" => Base::X,
            _ => return None,
        })
    }
}

/// Grammatical number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Number {
    /// `.s`.
    Sing,
    /// `.p`.
    Plur,
}

/// The verb forms the guide marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Form {
    /// `.pr`, finite present.
    Pr,
    /// `.pa`, finite past.
    Pa,
    /// `.fi`, finite with no tense.
    Fi,
    /// `.in`, infinitive.
    In,
    /// `.ing`, any -ing verb.
    Ing,
    /// `.pp`, past participle.
    Pp,
}

impl Form {
    fn suffix(self) -> &'static str {
        match self {
            Form::Pr => "pr",
            Form::Pa => "pa",
            Form::Fi => "fi",
            Form::In => "in",
            Form::Ing => "ing",
            Form::Pp => "pp",
        }
    }

    fn from_suffix(suffix: &str) -> Option<Form> {
        [Form::Pr, Form::Pa, Form::Fi, Form::In, Form::Ing, Form::Pp]
            .into_iter()
            .find(|form| form.suffix() == suffix)
    }

    /// The UD FEATS the guide's script writes for it, keys in alphabetical order.
    fn feats(self) -> &'static str {
        match self {
            Form::Pr => "Tense=Pres|VerbForm=Fin",
            Form::Pa => "Tense=Past|VerbForm=Fin",
            Form::Fi => "VerbForm=Fin",
            Form::In => "VerbForm=Inf",
            Form::Ing => "VerbForm=Ger",
            Form::Pp => "Tense=Past|VerbForm=Part",
        }
    }
}

/// A tag as the guide writes it: a base and, where the guide marks one, a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code {
    /// The part of speech.
    pub base: Base,
    /// The number, on `N`, `PN` and `PR`.
    pub number: Option<Number>,
    /// The verb form, on `V` and `AX`.
    pub form: Option<Form>,
}

impl Code {
    /// A code with no feature.
    pub fn bare(base: Base) -> Code {
        Code {
            base,
            number: None,
            form: None,
        }
    }

    /// Reads what a tagger wrote, strictly: the base must be one of the guide's, `N`, `PN`, `V` and
    /// `AX` must carry their feature, `PR` may carry a number, and nothing else may carry any.
    pub fn parse(text: &str) -> Result<Code, String> {
        let (base_text, feature) = match text.split_once('.') {
            Some((base, feature)) => (base, Some(feature)),
            None => (text, None),
        };
        let base = Base::from_code(base_text)
            .ok_or_else(|| format!("`{text}` is not a code of the guide"))?;
        let mut code = Code::bare(base);
        match feature {
            None if matches!(base, Base::N | Base::Pn) => {
                return Err(format!("`{text}` needs a number, `{text}.s` or `{text}.p`"));
            }
            None if base.takes_form() => {
                return Err(format!(
                    "`{text}` needs a verb form: .pr .pa .fi .in .ing or .pp"
                ));
            }
            None => {}
            Some(feature) if base.takes_number() => {
                code.number = Some(match feature {
                    "s" => Number::Sing,
                    "p" => Number::Plur,
                    _ => return Err(format!("`{text}`: a number is .s or .p")),
                });
            }
            Some(feature) if base.takes_form() => {
                code.form = Some(Form::from_suffix(feature).ok_or_else(|| {
                    format!("`{text}`: a verb form is .pr .pa .fi .in .ing or .pp")
                })?);
            }
            Some(_) => return Err(format!("`{text}`: `{base_text}` takes no feature")),
        }
        Ok(code)
    }

    /// Reads a tagger's UPOS and FEATS. A feature the answer does not give is left out, which is
    /// how a tagger that does not tag it abstains. `Err` for a UPOS that is not one of UD's, or
    /// `_`.
    pub fn from_conllu(upos: &str, feats: &str) -> Result<Code, String> {
        let base = Base::from_upos(upos).ok_or_else(|| {
            if upos == "_" {
                "no UPOS".to_string()
            } else {
                format!("UPOS `{upos}` is not one of the 17 UD tags")
            }
        })?;
        let pairs = conllu::pairs(feats);
        let has = |key: &str, value: &str| {
            pairs
                .iter()
                .any(|(k, v)| *k == key && v.split(',').any(|v| v == value))
        };
        let mut code = Code::bare(base);
        if base.takes_number() {
            code.number = if has("Number", "Sing") {
                Some(Number::Sing)
            } else if has("Number", "Plur") {
                Some(Number::Plur)
            } else {
                None
            };
        }
        if base.takes_form() {
            let given = pairs.iter().any(|(k, _)| *k == "VerbForm");
            code.form = if has("VerbForm", "Fin") {
                Some(if has("Tense", "Pres") {
                    Form::Pr
                } else if has("Tense", "Past") {
                    Form::Pa
                } else {
                    Form::Fi
                })
            } else if has("VerbForm", "Inf") {
                Some(Form::In)
            } else if has("VerbForm", "Ger") {
                Some(Form::Ing)
            } else if has("VerbForm", "Part") {
                Some(if has("Tense", "Pres") {
                    Form::Ing
                } else {
                    Form::Pp
                })
            } else if !given && has("Tense", "Pres") {
                Some(Form::Pr)
            } else if !given && has("Tense", "Past") {
                Some(Form::Pa)
            } else {
                None
            };
        }
        Ok(code)
    }

    /// The UPOS of `form` tagged this way.
    pub fn upos(&self, form: &str) -> &'static str {
        self.base.upos(form)
    }

    /// The FEATS column: `_` when the code has no feature.
    pub fn feats(&self) -> String {
        match (self.number, self.form) {
            (Some(Number::Sing), _) => "Number=Sing".to_string(),
            (Some(Number::Plur), _) => "Number=Plur".to_string(),
            (None, Some(form)) => form.feats().to_string(),
            (None, None) => "_".to_string(),
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.base.code())?;
        match (self.number, self.form) {
            (Some(Number::Sing), _) => f.write_str(".s"),
            (Some(Number::Plur), _) => f.write_str(".p"),
            (None, Some(form)) => write!(f, ".{}", form.suffix()),
            (None, None) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_of_the_guide_parses_and_prints_back() {
        for text in [
            "N.s", "N.p", "PN.s", "PN.p", "PR", "PR.s", "PR.p", "V.pr", "V.pa", "V.fi", "V.in",
            "V.ing", "V.pp", "AX.pr", "AX.pa", "AX.fi", "AX.in", "AX.ing", "AX.pp", "J", "R", "D",
            "P", "C", "T", "NM", "I", "X",
        ] {
            let code = Code::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"));
            assert_eq!(code.to_string(), text);
        }
    }

    #[test]
    fn a_bad_code_says_what_is_wrong() {
        for (text, says) in [
            ("Q", "not a code of the guide"),
            ("n.s", "not a code of the guide"),
            ("N", "needs a number"),
            ("PN", "needs a number"),
            ("V", "needs a verb form"),
            ("AX", "needs a verb form"),
            ("N.x", "a number is .s or .p"),
            ("PR.q", "a number is .s or .p"),
            ("V.s", "a verb form is"),
            ("D.s", "takes no feature"),
            ("X.p", "takes no feature"),
            ("N.s.p", "a number is"),
            ("", "not a code"),
            ("_", "not a code"),
        ] {
            let error = Code::parse(text).unwrap_err();
            assert!(error.contains(says), "{text}: {error}");
        }
    }

    #[test]
    fn codes_turn_into_upos_and_feats_as_the_guide_says() {
        let word = |text: &str, form: &str| {
            let code = Code::parse(text).unwrap();
            (code.upos(form), code.feats())
        };
        assert_eq!(word("N.s", "file"), ("NOUN", "Number=Sing".to_string()));
        assert_eq!(
            word("PN.p", "Actions"),
            ("PROPN", "Number=Plur".to_string())
        );
        assert_eq!(word("PR", "you"), ("PRON", "_".to_string()));
        assert_eq!(word("V.pr", "runs").1, "Tense=Pres|VerbForm=Fin");
        assert_eq!(word("V.pa", "ran").1, "Tense=Past|VerbForm=Fin");
        assert_eq!(word("AX.fi", "can").1, "VerbForm=Fin");
        assert_eq!(word("V.in", "run").1, "VerbForm=Inf");
        assert_eq!(word("V.ing", "running").1, "VerbForm=Ger");
        assert_eq!(word("V.pp", "run").1, "Tense=Past|VerbForm=Part");
        assert_eq!(word("T", "to").0, "PART");
        assert_eq!(word("NM", "one").0, "NUM");
        assert_eq!(word("I", "yes").0, "INTJ");
        assert_eq!(word("P", "in").0, "ADP");
        assert_eq!(word("J", "fast").0, "ADJ");
        assert_eq!(word("R", "very").0, "ADV");
        assert_eq!(word("D", "the").0, "DET");
        assert_eq!(word("X", "etc").0, "X");
    }

    #[test]
    fn c_is_a_coordinator_for_the_listed_words_and_a_subordinator_for_the_rest() {
        for form in [
            "and", "Or", "BUT", "nor", "yet", "plus", "both", "either", "neither",
        ] {
            assert_eq!(Base::C.upos(form), "CCONJ", "{form}");
        }
        for form in ["that", "if", "because", "while", "so", "as"] {
            assert_eq!(Base::C.upos(form), "SCONJ", "{form}");
        }
    }

    #[test]
    fn upos_and_feats_read_back_as_a_code() {
        let read = |upos: &str, feats: &str| Code::from_conllu(upos, feats).unwrap().to_string();
        assert_eq!(read("NOUN", "Number=Plur"), "N.p");
        assert_eq!(
            read("NOUN", "_"),
            "N",
            "a tagger that gives no number abstains"
        );
        assert_eq!(read("PROPN", "Number=Sing"), "PN.s");
        assert_eq!(read("PRON", "Case=Nom|Person=2|PronType=Prs"), "PR");
        assert_eq!(read("VERB", "Tense=Pres|VerbForm=Fin"), "V.pr");
        assert_eq!(read("VERB", "Mood=Imp|VerbForm=Fin"), "V.fi");
        assert_eq!(read("AUX", "VerbForm=Fin|Tense=Past"), "AX.pa");
        assert_eq!(read("VERB", "VerbForm=Inf"), "V.in");
        assert_eq!(read("VERB", "VerbForm=Ger"), "V.ing");
        assert_eq!(read("VERB", "Tense=Pres|VerbForm=Part"), "V.ing");
        assert_eq!(read("VERB", "Tense=Past|VerbForm=Part"), "V.pp");
        assert_eq!(read("VERB", "VerbForm=Part"), "V.pp");
        assert_eq!(
            read("VERB", "Tense=Past"),
            "V.pa",
            "no VerbForm, so tense decides"
        );
        assert_eq!(read("VERB", "_"), "V");
        assert_eq!(
            read("DET", "Number=Sing"),
            "D",
            "number is only read where the guide marks it"
        );
        assert_eq!(read("SCONJ", "_"), "C");
        assert_eq!(read("CCONJ", "_"), "C");
        assert_eq!(read("PART", "_"), "T");
        assert_eq!(read("PUNCT", "_"), "X");
        assert_eq!(read("SYM", "_"), "X");
        assert!(Code::from_conllu("_", "_").unwrap_err().contains("no UPOS"));
        assert!(
            Code::from_conllu("WORD", "_")
                .unwrap_err()
                .contains("17 UD tags")
        );
    }

    #[test]
    fn the_guide_s_feats_load_in_the_exam() {
        // The exam turns these FEATS into its flags; none may fail to load.
        for text in [
            "N.s", "PN.p", "PR.s", "V.pr", "V.pa", "V.fi", "V.in", "V.ing", "V.pp",
        ] {
            let code = Code::parse(text).unwrap();
            let feats = deslag_exam::tags::Features::from_ud(&code.feats()).unwrap();
            let expect = match text {
                "N.s" | "PR.s" => deslag_exam::tags::Features::SINGULAR,
                "PN.p" => deslag_exam::tags::Features::PLURAL,
                "V.pr" => {
                    deslag_exam::tags::Features::FINITE.union(deslag_exam::tags::Features::PRESENT)
                }
                "V.pa" => {
                    deslag_exam::tags::Features::FINITE.union(deslag_exam::tags::Features::PAST)
                }
                "V.fi" => deslag_exam::tags::Features::FINITE,
                "V.in" => deslag_exam::tags::Features::INFINITIVE,
                "V.ing" => deslag_exam::tags::Features::PRESENT_PARTICIPLE,
                _ => deslag_exam::tags::Features::PAST_PARTICIPLE,
            };
            assert_eq!(feats, expect, "{text}");
        }
    }
}
