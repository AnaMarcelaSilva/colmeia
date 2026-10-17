//! O texto antes dos fonemas: números por extenso, horas, siglas e símbolos
//! que a abelha fala do jeito que se lê ("14:30" vira "catorze e trinta",
//! "loja-web" vira "loja web"), e a divisão em frases curtas, que o Kokoro
//! fala melhor e começam a tocar mais cedo.

/// Um pedaço do texto: uma palavra (com a grafia original, para saber se era
/// sigla) ou uma pontuação que vira pausa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pedaco {
    Palavra(String),
    Pausa(char),
}

const UNIDADES: [&str; 20] = [
    "zero",
    "um",
    "dois",
    "três",
    "quatro",
    "cinco",
    "seis",
    "sete",
    "oito",
    "nove",
    "dez",
    "onze",
    "doze",
    "treze",
    "catorze",
    "quinze",
    "dezesseis",
    "dezessete",
    "dezoito",
    "dezenove",
];
const DEZENAS: [&str; 10] = ["", "", "vinte", "trinta", "quarenta", "cinquenta", "sessenta", "setenta", "oitenta", "noventa"];
const CENTENAS: [&str; 10] = ["", "cento", "duzentos", "trezentos", "quatrocentos", "quinhentos", "seiscentos", "setecentos", "oitocentos", "novecentos"];

/// Um número inteiro por extenso, até 999.999 (acima disso, dígito a dígito).
pub fn por_extenso(n: u64) -> String {
    match n {
        0..=19 => UNIDADES[n as usize].to_string(),
        20..=99 => {
            let (d, u) = (n / 10, n % 10);
            if u == 0 { DEZENAS[d as usize].to_string() } else { format!("{} e {}", DEZENAS[d as usize], UNIDADES[u as usize]) }
        }
        100 => "cem".to_string(),
        101..=999 => {
            let (c, r) = (n / 100, n % 100);
            if r == 0 { CENTENAS[c as usize].to_string() } else { format!("{} e {}", CENTENAS[c as usize], por_extenso(r)) }
        }
        1000..=999_999 => {
            let (m, r) = (n / 1000, n % 1000);
            let milhar = if m == 1 { "mil".to_string() } else { format!("{} mil", por_extenso(m)) };
            match r {
                0 => milhar,
                // "mil e duzentos", mas "mil duzentos e trinta".
                r if r < 100 || r % 100 == 0 => format!("{milhar} e {}", por_extenso(r)),
                r => format!("{milhar} {}", por_extenso(r)),
            }
        }
        _ => n.to_string().chars().map(|c| UNIDADES[c.to_digit(10).unwrap_or(0) as usize]).collect::<Vec<_>>().join(" "),
    }
}

/// As letras como se diz numa sigla ("API" vira "a pê i").
fn soletrar(sigla: &str) -> Vec<String> {
    sigla
        .chars()
        .map(|c| {
            match c.to_ascii_lowercase() {
                'a' => "á",
                'b' => "bê",
                'c' => "cê",
                'd' => "dê",
                'e' => "é",
                'f' => "éfe",
                'g' => "gê",
                'h' => "agá",
                'i' => "í",
                'j' => "jóta",
                'k' => "cá",
                'l' => "éle",
                'm' => "ême",
                'n' => "êne",
                'o' => "ó",
                'p' => "pê",
                'q' => "quê",
                'r' => "érre",
                's' => "ésse",
                't' => "tê",
                'u' => "ú",
                'v' => "vê",
                'w' => "dáblio",
                'x' => "xis",
                'y' => "ípsilon",
                'z' => "zê",
                _ => "",
            }
            .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Sigla é uma palavra curta toda em maiúsculas (API, SQL, PR), ou sem vogal.
fn eh_sigla(palavra: &str) -> bool {
    let letras: Vec<char> = palavra.chars().collect();
    let maiusculas = letras.len() >= 2 && letras.len() <= 5 && letras.iter().all(|c| c.is_ascii_uppercase());
    let sem_vogal = !letras.is_empty() && !letras.iter().any(|c| "aeiouyáéíóúâêôãõàAEIOUY".contains(*c));
    maiusculas || sem_vogal
}

/// Divide o texto em pedaços, já com os números e as siglas por extenso.
pub fn pedacos(texto: &str) -> Vec<Pedaco> {
    let mut saida = Vec::new();
    let mut palavra = String::new();
    let chars: Vec<char> = texto.chars().collect();
    let mut i = 0;
    let fechar = |palavra: &mut String, saida: &mut Vec<Pedaco>| {
        if palavra.is_empty() {
            return;
        }
        let p = std::mem::take(palavra);
        if eh_sigla(&p) {
            saida.extend(soletrar(&p).into_iter().map(Pedaco::Palavra));
        } else {
            saida.push(Pedaco::Palavra(p));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            fechar(&mut palavra, &mut saida);
            let inicio = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let numero: String = chars[inicio..i].iter().collect();
            // Hora: 14:30 vira "catorze e trinta" (e 9:05, "nove e cinco").
            if chars.get(i) == Some(&':') && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) && chars.get(i + 2).is_some_and(|c| c.is_ascii_digit()) {
                let minutos: u64 = chars[i + 1..i + 3].iter().collect::<String>().parse().unwrap_or(0);
                let horas: u64 = numero.parse().unwrap_or(0);
                saida.extend(por_extenso(horas).split(' ').map(|p| Pedaco::Palavra(p.to_string())));
                if minutos > 0 {
                    saida.push(Pedaco::Palavra("e".into()));
                    saida.extend(por_extenso(minutos).split(' ').map(|p| Pedaco::Palavra(p.to_string())));
                }
                i += 3;
                continue;
            }
            let n: u64 = numero.parse().unwrap_or(0);
            saida.extend(por_extenso(n).split(' ').map(|p| Pedaco::Palavra(p.to_string())));
            continue;
        }
        if c.is_alphabetic() {
            palavra.push(c);
        } else if c == '\'' && !palavra.is_empty() {
            // "d'água": o apóstrofo some.
        } else {
            fechar(&mut palavra, &mut saida);
            match c {
                '.' | ',' | ';' | ':' | '!' | '?' => saida.push(Pedaco::Pausa(c)),
                '…' => saida.push(Pedaco::Pausa('.')),
                '%' => saida.extend(["por", "cento"].map(|p| Pedaco::Palavra(p.into()))),
                '&' => saida.push(Pedaco::Palavra("e".into())),
                '+' => saida.push(Pedaco::Palavra("mais".into())),
                '@' => saida.push(Pedaco::Palavra("arroba".into())),
                // Travessão e parênteses viram uma pausa curta.
                '—' | '–' | '(' | ')' => saida.push(Pedaco::Pausa(',')),
                // Hífen, sublinhado, barra e o resto separam palavras.
                _ => {}
            }
        }
        i += 1;
    }
    fechar(&mut palavra, &mut saida);
    // Pausas repetidas ou no começo não ajudam.
    let mut limpos: Vec<Pedaco> = Vec::with_capacity(saida.len());
    for p in saida {
        if matches!(p, Pedaco::Pausa(_)) && matches!(limpos.last(), None | Some(Pedaco::Pausa(_))) {
            continue;
        }
        limpos.push(p);
    }
    limpos
}

/// Divide o texto em frases (no ponto, na exclamação e na interrogação), com
/// a pontuação junto. Uma frase longa demais é cortada nas vírgulas.
pub fn frases(texto: &str, limite: usize) -> Vec<String> {
    let mut saida = Vec::new();
    let mut atual = String::new();
    for c in texto.chars() {
        atual.push(c);
        if matches!(c, '.' | '!' | '?' | '\n') && atual.trim().chars().count() > 1 {
            saida.push(std::mem::take(&mut atual));
        }
    }
    if !atual.trim().is_empty() {
        saida.push(atual);
    }
    saida.into_iter().flat_map(|f| cortar(f.trim(), limite)).filter(|f| f.chars().any(|c| c.is_alphanumeric())).collect()
}

fn cortar(frase: &str, limite: usize) -> Vec<String> {
    if frase.chars().count() <= limite {
        return vec![frase.to_string()];
    }
    let mut partes = Vec::new();
    let mut atual = String::new();
    for palavra in frase.split_inclusive([',', ';', ' ']) {
        if atual.chars().count() + palavra.chars().count() > limite && !atual.is_empty() {
            partes.push(std::mem::take(&mut atual).trim().to_string());
        }
        atual.push_str(palavra);
    }
    if !atual.trim().is_empty() {
        partes.push(atual.trim().to_string());
    }
    partes
}

#[cfg(test)]
mod testes {
    use super::*;

    fn palavras(t: &str) -> String {
        pedacos(t)
            .into_iter()
            .map(|p| match p {
                Pedaco::Palavra(s) => s,
                Pedaco::Pausa(c) => c.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn numeros_por_extenso() {
        assert_eq!(por_extenso(0), "zero");
        assert_eq!(por_extenso(14), "catorze");
        assert_eq!(por_extenso(21), "vinte e um");
        assert_eq!(por_extenso(100), "cem");
        assert_eq!(por_extenso(101), "cento e um");
        assert_eq!(por_extenso(345), "trezentos e quarenta e cinco");
        assert_eq!(por_extenso(1000), "mil");
        assert_eq!(por_extenso(1200), "mil e duzentos");
        assert_eq!(por_extenso(2024), "dois mil e vinte e quatro");
        assert_eq!(por_extenso(1234), "mil duzentos e trinta e quatro");
    }

    #[test]
    fn horas_siglas_e_simbolos() {
        assert_eq!(palavras("Desde 14:30."), "Desde catorze e trinta .");
        assert_eq!(palavras("às 9:00"), "às nove");
        assert_eq!(palavras("o projeto loja-web"), "o projeto loja web");
        assert_eq!(palavras("api_pedidos/v2"), "api pedidos vê dois");
        assert_eq!(palavras("A API caiu"), "A á pê í caiu");
        assert_eq!(palavras("50% pronto"), "cinquenta por cento pronto");
        assert_eq!(palavras("3 tarefas — duas paradas"), "três tarefas , duas paradas");
    }

    #[test]
    fn pausas_nao_se_repetem() {
        assert_eq!(palavras("... Olá!! Tudo bem?"), "Olá ! Tudo bem ?");
    }

    #[test]
    fn divide_em_frases_e_corta_as_longas() {
        assert_eq!(frases("Rodando a loja. Pronto! E agora?", 200), vec!["Rodando a loja.", "Pronto!", "E agora?"]);
        let longa = "um, dois, três, quatro, cinco, seis, sete, oito, nove, dez";
        let partes = frases(longa, 20);
        assert!(partes.len() > 2 && partes.iter().all(|p| p.chars().count() <= 20), "{partes:?}");
        assert!(frases("...", 50).is_empty());
    }
}
