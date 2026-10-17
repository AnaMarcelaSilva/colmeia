//! Do português do Brasil aos fonemas que o Kokoro lê: o alfabeto fonético
//! do espeak-ng para pt-BR, com que ele aprendeu. Regras de leitura (sílaba
//! tônica, vogais abertas e nasais, "t" e "d" antes de "i", "r" forte, "s"
//! entre vogais, "l" no fim da sílaba) e um vocabulário curto com o que as
//! regras erram: palavras com "é" ou "ó" sem acento escrito, as palavras
//! átonas e os termos em inglês que aparecem nas respostas da Colmeia.
//!
//! Escrito aqui (e não o espeak-ng) porque o espeak-ng é GPL: a Colmeia só
//! usa bibliotecas compatíveis com MIT e Apache-2.0.

use super::texto::Pedaco;

/// A marca de sílaba tônica, antes da vogal (como o espeak-ng escreve).
const TONICA: char = 'ˈ';

/// Palavras que as regras não acertam, já em fonemas.
fn vocabulario(palavra: &str) -> Option<&'static str> {
    Some(match palavra {
        // Átonas: artigos, preposições, pronomes e conjunções.
        "a" => "a",
        "o" => "ʊ",
        "os" => "ʊs",
        "as" => "ɐs",
        "e" => "i",
        "é" => "ˈɛ",
        "de" => "ʤi",
        "do" => "du",
        "da" => "dɐ",
        "dos" => "dus",
        "das" => "dɐs",
        "no" => "nu",
        "na" => "nɐ",
        "nos" => "nus",
        "nas" => "nɐs",
        "em" => "ẽɪ̃",
        "com" => "kõ",
        "sem" => "sẽɪ̃",
        "que" => "ki",
        "se" => "si",
        "me" => "mi",
        "te" => "ʧi",
        "lhe" => "ʎi",
        "por" => "puɾ",
        "para" => "ˈpaɾɐ",
        "pra" => "pɾɐ",
        "ao" => "aʊ",
        "aos" => "aʊs",
        "à" => "a",
        "às" => "as",
        "um" => "ˈũ",
        "uns" => "ˈũs",
        "ou" => "ˈow",
        "nem" => "nẽɪ̃",
        // "é" e "ó" abertos sem acento escrito, e o "x" de táxi.
        "colmeia" => "kowmˈɛɪɐ",
        "colmeias" => "kowmˈɛɪɐs",
        "tarefa" => "taɾˈɛfɐ",
        "tarefas" => "taɾˈɛfɐs",
        "projeto" => "pɾoʒˈɛtʊ",
        "projetos" => "pɾoʒˈɛtʊs",
        "agora" => "aɡˈɔɾɐ",
        "hora" => "ˈɔɾɐ",
        "horas" => "ˈɔɾɐs",
        "nova" => "nˈɔvɐ",
        "novas" => "nˈɔvɐs",
        "ela" => "ˈɛlɐ",
        "elas" => "ˈɛlɐs",
        "ele" => "ˈeli",
        "espera" => "ispˈɛɾɐ",
        "esperam" => "ispˈɛɾɐ̃ʊ̃",
        "certo" => "sˈɛɾtʊ",
        "certa" => "sˈɛɾtɐ",
        "pede" => "pˈɛʤi",
        "pedem" => "pˈɛdẽɪ̃",
        "teste" => "tˈɛsʧi",
        "testes" => "tˈɛsʧis",
        "zero" => "zˈɛɾʊ",
        "dez" => "dˈɛs",
        "sete" => "sˈɛʧi",
        "nove" => "nˈɔvi",
        "porta" => "pˈɔɾtɐ",
        "volta" => "vˈɔwtɐ",
        "nossa" => "nˈɔsɐ",
        "nosso" => "nˈɔsʊ",
        "loja" => "lˈɔʒɐ",
        "lojas" => "lˈɔʒɐs",
        "ok" => "ɔkˈeɪ",
        // Termos em inglês das respostas.
        "google" => "ɡˈuɡow",
        "login" => "lˈoɡĩ",
        "logout" => "loɡˈawʧi",
        "web" => "wˈɛb",
        "app" => "ˈɛpi",
        "claude" => "klˈɔʤi",
        "code" => "kˈowʤi",
        "codex" => "kˈɔdɛks",
        "gemini" => "ʤˈɛmini",
        "play" => "plˈeɪ",
        "backlog" => "bˈɛklɔɡ",
        "deploy" => "ʤiplˈɔɪ",
        "git" => "ɡˈiʧi",
        "commit" => "kˈɔmiʧi",
        "branch" => "bɾˈɛnʃi",
        "cache" => "kˈaʃi",
        "rust" => "xˈɐsʧi",
        "front" => "fɾˈõʧi",
        "frontend" => "fɾõʧˈẽdi",
        "backend" => "bɛkˈẽdi",
        "api" => "apˈi",
        "shell" => "ʃˈɛw",
        "online" => "õnlˈaɪni",
        "site" => "sˈaɪʧi",
        "bug" => "bˈɐɡi",
        "bugs" => "bˈɐɡis",
        "script" => "iskɾˈipʧi",
        "docker" => "dˈɔkeɾ",
        "táxi" => "tˈaksi",
        _ => return None,
    })
}

fn eh_vogal(c: char) -> bool {
    "aeiouáéíóúâêôãõàüy".contains(c)
}

fn com_acento(c: char) -> bool {
    "áéíóúâêô".contains(c)
}

/// Fonemas de uma palavra (já sem números nem símbolos).
pub fn palavra(original: &str) -> String {
    let p = original.to_lowercase();
    if let Some(f) = vocabulario(&p) {
        return decompor(f);
    }
    let letras: Vec<char> = p.chars().filter(|c| c.is_alphabetic()).collect();
    if letras.is_empty() {
        return String::new();
    }
    decompor(&Leitura::nova(letras).ler())
}

/// O Kokoro lê o til separado da vogal (ẽ é "e" e o til que combina).
fn decompor(s: &str) -> String {
    let mut saida = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            'ẽ' => saida.push_str("e\u{303}"),
            'õ' => saida.push_str("o\u{303}"),
            'ĩ' => saida.push_str("i\u{303}"),
            'ũ' => saida.push_str("u\u{303}"),
            'ã' => saida.push_str("a\u{303}"),
            _ => saida.push(c),
        }
    }
    saida
}

/// Uma frase: as palavras separadas por espaço e a pontuação junto, como o
/// espeak-ng entrega.
pub fn frase(pedacos: &[Pedaco]) -> String {
    let mut saida = String::new();
    for p in pedacos {
        match p {
            Pedaco::Palavra(w) => {
                let f = palavra(w);
                if f.is_empty() {
                    continue;
                }
                if !saida.is_empty() {
                    saida.push(' ');
                }
                saida.push_str(&f);
            }
            Pedaco::Pausa(c) => saida.push(*c),
        }
    }
    saida
}

/// Uma vogal (ou ditongo) de uma sílaba: de onde a onde vai nas letras.
#[derive(Clone, Copy, Debug)]
struct Nucleo {
    inicio: usize,
    fim: usize,
}

struct Leitura {
    l: Vec<char>,
    nucleos: Vec<Nucleo>,
    tonico: usize,
    /// O "u" depois de "q" ou "g": "w" antes de "a" e "o"; mudo antes de "e" e "i".
    u_apoio: Vec<bool>,
}

impl Leitura {
    fn nova(l: Vec<char>) -> Leitura {
        let n = l.len();
        let mut u_apoio = vec![false; n];
        for i in 1..n {
            if l[i] == 'u' && matches!(l[i - 1], 'q' | 'g') && i + 1 < n && eh_vogal(l[i + 1]) {
                u_apoio[i] = true;
            }
        }
        let mut leitura = Leitura { l, nucleos: Vec::new(), tonico: 0, u_apoio };
        leitura.nucleos = leitura.silabas();
        leitura.tonico = leitura.tonica();
        leitura
    }

    fn c(&self, i: usize) -> Option<char> {
        self.l.get(i).copied()
    }

    fn vogal_em(&self, i: usize) -> bool {
        self.c(i).is_some_and(eh_vogal) && !self.u_apoio.get(i).copied().unwrap_or(false)
    }

    /// As vogais de cada sílaba, com os ditongos decrescentes (ai, ei, ou,
    /// ão, õe…) juntos e os hiatos separados.
    fn silabas(&self) -> Vec<Nucleo> {
        let n = self.l.len();
        let mut nucleos = Vec::new();
        let mut i = 0;
        while i < n {
            if !self.vogal_em(i) {
                i += 1;
                continue;
            }
            let inicio = i;
            let c = self.l[i];
            i += 1;
            if let Some(prox) = self.c(i) {
                let nasal_dupla = matches!((c, prox), ('ã', 'o' | 'e') | ('õ', 'e'));
                let semivogal = matches!(prox, 'i' | 'u') && !matches!(c, 'i' | 'u' | 'í' | 'ú') || (c == 'u' && prox == 'i');
                // "ainda", "rainha", "cair", "juiz": a vogal fraca leva a sílaba.
                let depois = i + 1;
                let hiato = self.c(depois) == Some('n') && self.c(depois + 1) == Some('h')
                    || self.c(depois).is_some_and(|d| "mnrlz".contains(d)) && self.c(depois + 1).is_none_or(|e| !eh_vogal(e) && e != 'h');
                if nasal_dupla || semivogal && !hiato {
                    i += 1;
                }
            }
            nucleos.push(Nucleo { inicio, fim: i });
        }
        nucleos
    }

    /// A sílaba tônica: a do acento escrito; sem acento, a penúltima nas
    /// palavras terminadas em a, e, o (com s), am, em e ens; senão a última.
    fn tonica(&self) -> usize {
        let total = self.nucleos.len();
        if total <= 1 {
            return 0;
        }
        let tem = |nuc: &Nucleo, f: fn(char) -> bool| self.l[nuc.inicio..nuc.fim].iter().any(|c| f(*c));
        if let Some(i) = self.nucleos.iter().position(|nuc| tem(nuc, com_acento)) {
            return i;
        }
        if let Some(i) = self.nucleos.iter().position(|nuc| tem(nuc, |c| c == 'ã' || c == 'õ')) {
            return i;
        }
        let fim: String = self.l.iter().rev().take(3).rev().collect();
        let paroxitona = ["a", "e", "o", "as", "es", "os", "am", "em", "ens"].iter().any(|t| fim.ends_with(t));
        if paroxitona { total - 2 } else { total - 1 }
    }

    /// O "m" ou "n" depois do núcleo fecha a sílaba (e só nasaliza): antes de
    /// consoante (que não seja "h") ou no fim da palavra.
    fn nasal_fechada(&self, nuc: Nucleo) -> bool {
        matches!(self.c(nuc.fim), Some('m' | 'n')) && self.c(nuc.fim + 1).is_none_or(|d| !eh_vogal(d) && d != 'h')
    }

    /// A sílaba tônica seguida de m, n ou nh (cama, banana, sonho) soa nasal.
    fn nasal_aberta(&self, k: usize, nuc: Nucleo) -> bool {
        k == self.tonico
            && match self.c(nuc.fim) {
                Some('m' | 'n') => self.c(nuc.fim + 1).is_some_and(|d| eh_vogal(d) || d == 'h'),
                _ => false,
            }
    }

    /// A última sílaba, átona: o "a" vira ɐ, o "e" vira i e o "o" vira ʊ.
    fn final_atona(&self, k: usize, nuc: Nucleo) -> bool {
        if k != self.nucleos.len() - 1 || k == self.tonico {
            return false;
        }
        let resto: String = self.l[nuc.fim..].iter().collect();
        matches!(resto.as_str(), "" | "s" | "m" | "ns")
    }

    fn vogal(&self, c: char, tonica: bool, nasal: bool, final_atona: bool) -> &'static str {
        match (c, nasal) {
            ('a' | 'à' | 'á' | 'â' | 'ã', true) | ('ã', _) => "ɐ̃",
            ('â', false) => "ɐ",
            ('á' | 'à', false) => "a",
            ('a', false) if final_atona => "ɐ",
            ('a', false) => "a",
            ('é', false) => "ɛ",
            ('e' | 'é' | 'ê', true) => "ẽ",
            ('ê', false) => "e",
            ('e', false) if final_atona && !tonica => "i",
            ('e', false) => "e",
            ('ó', false) => "ɔ",
            ('o' | 'ó' | 'ô' | 'õ', true) | ('õ', _) => "õ",
            ('ô', false) => "o",
            ('o', false) if final_atona && !tonica => "ʊ",
            ('o', false) => "o",
            ('i' | 'í' | 'y', true) => "ĩ",
            ('i' | 'í' | 'y', false) => "i",
            ('u' | 'ú' | 'ü', true) => "ũ",
            _ => "u",
        }
    }

    /// A segunda vogal de um ditongo: i e u viram semivogais.
    fn semivogal(c: char, nasal: bool) -> &'static str {
        match (c, nasal) {
            ('i' | 'e', true) => "ɪ̃",
            ('u' | 'o', true) => "ʊ̃",
            ('i' | 'e' | 'í', false) => "ɪ",
            _ => "ʊ",
        }
    }

    /// O som de vogal que vem depois da letra i (para t e d antes de i: ʧ, ʤ).
    fn soa_i(&self, i: usize) -> bool {
        let Some(k) = self.nucleos.iter().position(|nuc| nuc.inicio == i) else {
            return false;
        };
        let nuc = self.nucleos[k];
        match self.l[i] {
            'i' | 'í' | 'y' => true,
            'e' => self.final_atona(k, nuc) && !self.nasal_fechada(nuc),
            _ => false,
        }
    }

    fn ler(&self) -> String {
        let n = self.l.len();
        let mut s = String::new();
        let mut i = 0;
        while i < n {
            if let Some(k) = self.nucleos.iter().position(|nuc| nuc.inicio == i) {
                i = self.ler_nucleo(k, &mut s);
                continue;
            }
            let c = self.l[i];
            let prox = self.c(i + 1);
            let ant = if i > 0 { self.c(i - 1) } else { None };
            let entre_vogais = ant.is_some_and(eh_vogal) && prox.is_some_and(eh_vogal);
            let fim_de_silaba = prox.is_none_or(|p| !eh_vogal(p));
            match c {
                'b' | 'f' | 'k' | 'p' | 'v' => s.push(c),
                'd' => s.push_str(if self.soa_i(i + 1) { "ʤ" } else { "d" }),
                't' => s.push_str(if self.soa_i(i + 1) { "ʧ" } else { "t" }),
                'c' if prox == Some('h') => {
                    s.push('ʃ');
                    i += 1;
                }
                'c' if prox.is_some_and(|p| "eiéêíy".contains(p)) => s.push('s'),
                'c' | 'q' => s.push('k'),
                'ç' => s.push('s'),
                'g' if prox.is_some_and(|p| "eiéêíy".contains(p)) => s.push('ʒ'),
                'g' => s.push('ɡ'),
                'j' => s.push('ʒ'),
                'h' => {}
                'l' if prox == Some('h') => {
                    s.push('ʎ');
                    i += 1;
                }
                'l' if fim_de_silaba => s.push('w'),
                'l' => s.push('l'),
                'm' => s.push('m'),
                'n' if prox == Some('h') => {
                    s.push('ɲ');
                    i += 1;
                }
                'n' => s.push('n'),
                'r' if prox == Some('r') => {
                    s.push('x');
                    i += 1;
                }
                'r' if i == 0 || ant.is_some_and(|a| "nls".contains(a)) => s.push('x'),
                'r' => s.push('ɾ'),
                's' if prox == Some('s') => {
                    s.push('s');
                    i += 1;
                }
                's' if prox == Some('c') && self.c(i + 2).is_some_and(|p| "eiéêí".contains(p)) => {
                    s.push('s');
                    i += 1;
                }
                's' if entre_vogais => s.push('z'),
                's' if prox.is_some_and(|p| "bdglmnrv".contains(p)) => s.push('z'),
                's' => s.push('s'),
                'x' if i + 1 == n => s.push_str("ks"),
                'x' if ant == Some('e') && i == 1 && prox.is_some_and(eh_vogal) => s.push('z'),
                'x' if ant == Some('e') && i == 1 => s.push('s'),
                'x' => s.push('ʃ'),
                'z' if i + 1 == n => s.push('s'),
                'z' => s.push('z'),
                'w' => s.push('w'),
                // "quando", "água": w; "quero", "guia": mudo.
                'u' if self.u_apoio[i] && prox.is_some_and(|p| "aoáóâôãõ".contains(p)) => s.push('w'),
                _ => {}
            }
            i += 1;
        }
        s
    }

    /// Escreve as vogais de um núcleo (com a marca de tônica) e devolve onde
    /// a leitura continua (depois do m ou n que só nasalizou).
    fn ler_nucleo(&self, k: usize, s: &mut String) -> usize {
        let nuc = self.nucleos[k];
        let tonica = k == self.tonico && self.nucleos.len() > 1 || self.nucleos.len() == 1;
        let fechada = self.nasal_fechada(nuc);
        let nasal = fechada || self.nasal_aberta(k, nuc);
        let final_atona = self.final_atona(k, nuc);
        if tonica {
            s.push(TONICA);
        }
        let primeira = self.l[nuc.inicio];
        let dupla = nuc.fim - nuc.inicio > 1;
        s.push_str(self.vogal(primeira, tonica, nasal || "ãõ".contains(primeira), final_atona));
        if dupla {
            let segunda = self.l[nuc.inicio + 1];
            let ditongo_nasal = "ãõ".contains(primeira) || nasal;
            s.push_str(Self::semivogal(segunda, ditongo_nasal));
        } else if fechada && self.c(nuc.fim) == Some('m') && nuc.fim + 1 == self.l.len() {
            // "-am" e "-em" no fim viram ditongo: falam, também, homem.
            match primeira {
                'a' | 'â' => s.push_str("ʊ̃"),
                'e' | 'é' | 'ê' => s.push_str("ɪ̃"),
                _ => {}
            }
        } else if fechada && self.c(nuc.fim) == Some('n') && self.c(nuc.fim + 1) == Some('s') && nuc.fim + 2 == self.l.len() && "eéê".contains(primeira) {
            s.push_str("ɪ̃");
        }
        if fechada { nuc.fim + 1 } else { nuc.fim }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::fala::texto::pedacos;

    #[test]
    fn le_palavras_comuns() {
        let casos = [
            ("casa", "kˈazɐ"),
            ("rodar", "xodˈaɾ"),
            ("rodando", "xodˈɐ̃dʊ"),
            ("trabalhando", "tɾabaʎˈɐ̃dʊ"),
            ("aprovação", "apɾovasˈɐ̃ʊ̃"),
            ("não", "nˈɐ̃ʊ̃"),
            ("mãe", "mˈɐ̃ɪ̃"),
            ("também", "tɐ̃bˈẽɪ̃"),
            ("bem", "bˈẽɪ̃"),
            ("homem", "ˈõmẽɪ̃"),
            ("tarde", "tˈaɾʤi"),
            ("gente", "ʒˈẽʧi"),
            ("dia", "ʤˈiɐ"),
            ("muito", "mˈuɪtʊ"),
            ("ainda", "aˈĩdɐ"),
            ("rainha", "xaˈĩɲɐ"),
            ("quando", "kwˈɐ̃dʊ"),
            ("quente", "kˈẽʧi"),
            ("guitarra", "ɡitˈaxɐ"),
            ("chave", "ʃˈavi"),
            ("filho", "fˈiʎʊ"),
            ("brasil", "bɾazˈiw"),
            ("mesmo", "mˈezmʊ"),
            ("exemplo", "ezˈẽplʊ"),
            ("número", "nˈũmeɾʊ"),
            ("você", "vosˈe"),
            ("está", "estˈa"),
            ("cama", "kˈɐ̃mɐ"),
            ("isso", "ˈisʊ"),
            ("luz", "lˈus"),
            ("pronto", "pɾˈõtʊ"),
            ("revisão", "xevizˈɐ̃ʊ̃"),
            ("concluído", "kõkluˈidʊ"),
            ("água", "ˈaɡwɐ"),
            ("nascer", "nasˈeɾ"),
        ];
        for (p, esperado) in casos {
            assert_eq!(palavra(p), decompor(esperado), "{p}");
        }
    }

    #[test]
    fn vocabulario_e_maiusculas() {
        assert_eq!(palavra("Colmeia"), "kowmˈɛɪɐ");
        assert_eq!(palavra("Google"), "ɡˈuɡow");
        assert_eq!(palavra("TAREFA"), "taɾˈɛfɐ");
    }

    #[test]
    fn frase_com_pausas() {
        let f = frase(&pedacos("Rodando o projeto loja-web."));
        assert_eq!(f, decompor("xodˈɐ̃dʊ ʊ pɾoʒˈɛtʊ lˈɔʒɐ wˈɛb."));
        let f = frase(&pedacos("Não, 2 tarefas!"));
        assert_eq!(f, decompor("nˈɐ̃ʊ̃, dˈoɪs taɾˈɛfɐs!"));
    }

    #[test]
    fn so_letras_do_vocabulario_do_kokoro() {
        let vocab = super::super::kokoro::VOCABULARIO;
        let texto = "Não entendi. Você pode pedir: rodar ou parar um projeto, abrir ou encerrar uma tarefa, como está uma tarefa e o que espera por você. Ação, coração, pão, mãe, põe, órgão, ônibus, xícara, chuva, lhama, nhoque, quilo, guerra, ninguém, tóxico, exceção, ascensão, Zuzu, Wagner, Yasmin, Kátia.";
        for c in frase(&pedacos(texto)).chars() {
            assert!(c == ' ' || vocab.contains(&c), "{c:?} não está no vocabulário do Kokoro");
        }
    }
}
