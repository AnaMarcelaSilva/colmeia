//! O markdown das notas: um subconjunto pequeno, lido aqui mesmo (sem
//! dependência). Títulos (#, ##, ###), **negrito**, *itálico*, `código`,
//! listas (-, *, 1.), caixas (- [ ] e - [x], só desenho), tabelas em pipe
//! (com alinhamento :--:) e blocos cercados por três crases.
//!
//! Sem links, sem HTML e sem imagens por URL: nada abre a rede nem arquivos.
//! O que não é reconhecido fica como texto, do jeito que foi escrito.

/// Um pedaço de texto com a formatação dele.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trecho {
    pub texto: String,
    pub negrito: bool,
    pub italico: bool,
    pub codigo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marcador {
    Ponto,
    /// "1." (o número como foi escrito).
    Numero(u32),
    Caixa(bool),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Alinhamento {
    #[default]
    Esquerda,
    Centro,
    Direita,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Bloco {
    /// Nível 1 a 3.
    Titulo(u8, Vec<Trecho>),
    /// Linhas seguidas; cada quebra de linha fica (a nota é escrita como se lê).
    Paragrafo(Vec<Vec<Trecho>>),
    Item {
        nivel: u8,
        marcador: Marcador,
        trechos: Vec<Trecho>,
    },
    Codigo(String),
    Tabela {
        alinhamentos: Vec<Alinhamento>,
        cabecalho: Option<Vec<Vec<Trecho>>>,
        linhas: Vec<Vec<Vec<Trecho>>>,
    },
    /// Uma linha em branco entre blocos (só o espaço).
    Vazio,
}

/// Lê o texto da nota em blocos.
pub fn ler(texto: &str) -> Vec<Bloco> {
    let linhas: Vec<&str> = texto.lines().collect();
    let mut blocos = Vec::new();
    let mut i = 0;
    let mut paragrafo: Vec<Vec<Trecho>> = Vec::new();
    let fechar = |paragrafo: &mut Vec<Vec<Trecho>>, blocos: &mut Vec<Bloco>| {
        if !paragrafo.is_empty() {
            blocos.push(Bloco::Paragrafo(std::mem::take(paragrafo)));
        }
    };
    while i < linhas.len() {
        let linha = linhas[i];
        let sem_recuo = linha.trim_start();
        if sem_recuo.starts_with("```") {
            fechar(&mut paragrafo, &mut blocos);
            let mut codigo = Vec::new();
            i += 1;
            while i < linhas.len() && !linhas[i].trim_start().starts_with("```") {
                codigo.push(linhas[i]);
                i += 1;
            }
            blocos.push(Bloco::Codigo(codigo.join("\n")));
            i += 1;
            continue;
        }
        if sem_recuo.is_empty() {
            fechar(&mut paragrafo, &mut blocos);
            if !matches!(blocos.last(), Some(Bloco::Vazio) | None) {
                blocos.push(Bloco::Vazio);
            }
            i += 1;
            continue;
        }
        if let Some((nivel, resto)) = titulo(sem_recuo) {
            fechar(&mut paragrafo, &mut blocos);
            blocos.push(Bloco::Titulo(nivel, trechos(resto)));
            i += 1;
            continue;
        }
        if sem_recuo.starts_with('|') {
            fechar(&mut paragrafo, &mut blocos);
            let mut tabela = Vec::new();
            while i < linhas.len() && linhas[i].trim_start().starts_with('|') {
                tabela.push(linhas[i].trim());
                i += 1;
            }
            blocos.push(ler_tabela(&tabela));
            continue;
        }
        if let Some((marcador, resto)) = item(sem_recuo) {
            fechar(&mut paragrafo, &mut blocos);
            let recuo = linha.len() - sem_recuo.len();
            let nivel = (recuo / 2).min(4) as u8;
            blocos.push(Bloco::Item { nivel, marcador, trechos: trechos(resto) });
            i += 1;
            continue;
        }
        paragrafo.push(trechos(linha.trim_end()));
        i += 1;
    }
    fechar(&mut paragrafo, &mut blocos);
    while matches!(blocos.last(), Some(Bloco::Vazio)) {
        blocos.pop();
    }
    blocos
}

fn titulo(linha: &str) -> Option<(u8, &str)> {
    let hashes = linha.chars().take_while(|c| *c == '#').count();
    if !(1..=3).contains(&hashes) {
        return None;
    }
    let resto = &linha[hashes..];
    resto.strip_prefix(' ').map(|r| (hashes as u8, r.trim()))
}

fn item(linha: &str) -> Option<(Marcador, &str)> {
    for prefixo in ["- ", "* ", "+ "] {
        if let Some(resto) = linha.strip_prefix(prefixo) {
            for (caixa, marcada) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
                if let Some(r) = resto.strip_prefix(caixa) {
                    return Some((Marcador::Caixa(marcada), r));
                }
            }
            if resto == "[ ]" || resto == "[x]" || resto == "[X]" {
                return Some((Marcador::Caixa(resto != "[ ]"), ""));
            }
            return Some((Marcador::Ponto, resto));
        }
    }
    let digitos = linha.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=4).contains(&digitos) {
        let resto = &linha[digitos..];
        if let Some(r) = resto.strip_prefix(". ").or_else(|| resto.strip_prefix(") ")) {
            return Some((Marcador::Numero(linha[..digitos].parse().unwrap_or(1)), r));
        }
    }
    None
}

/// As células de uma linha de tabela: "| a | b |" vira ["a", "b"].
fn celulas(linha: &str) -> Vec<&str> {
    let dentro = linha.trim().strip_prefix('|').unwrap_or(linha);
    let dentro = dentro.strip_suffix('|').unwrap_or(dentro);
    dentro.split('|').map(str::trim).collect()
}

fn separador(linha: &str) -> Option<Vec<Alinhamento>> {
    let partes = celulas(linha);
    let mut alinhamentos = Vec::new();
    for p in partes {
        let miolo = p.trim_matches(':');
        if miolo.is_empty() || !miolo.chars().all(|c| c == '-') {
            return None;
        }
        alinhamentos.push(match (p.starts_with(':'), p.ends_with(':')) {
            (true, true) => Alinhamento::Centro,
            (false, true) => Alinhamento::Direita,
            _ => Alinhamento::Esquerda,
        });
    }
    Some(alinhamentos)
}

fn ler_tabela(linhas: &[&str]) -> Bloco {
    let (cabecalho, alinhamentos, resto) = match linhas.get(1).and_then(|l| separador(l)) {
        Some(alinhamentos) => (Some(celulas(linhas[0]).into_iter().map(trechos).collect::<Vec<_>>()), alinhamentos, &linhas[2..]),
        None => (None, Vec::new(), linhas),
    };
    let linhas: Vec<Vec<Vec<Trecho>>> = resto.iter().map(|l| celulas(l).into_iter().map(trechos).collect()).collect();
    let colunas = cabecalho.as_ref().map_or(0, Vec::len).max(linhas.iter().map(Vec::len).max().unwrap_or(0)).max(alinhamentos.len());
    let mut alinhamentos = alinhamentos;
    alinhamentos.resize(colunas, Alinhamento::Esquerda);
    Bloco::Tabela { alinhamentos, cabecalho, linhas }
}

/// Os trechos de uma linha: **negrito**, *itálico* e `código`. Um marcador
/// sem par fica como texto.
pub fn trechos(linha: &str) -> Vec<Trecho> {
    let mut saida: Vec<Trecho> = Vec::new();
    let mut atual = String::new();
    let (mut negrito, mut italico) = (false, false);
    let chars: Vec<char> = linha.chars().collect();
    let mut i = 0;
    let empurrar = |saida: &mut Vec<Trecho>, atual: &mut String, negrito: bool, italico: bool, codigo: bool| {
        if !atual.is_empty() {
            saida.push(Trecho { texto: std::mem::take(atual), negrito, italico, codigo });
        }
    };
    let fecha = |de: usize, marca: &[char]| -> bool {
        // Há o marcador de fechar mais adiante na linha.
        let n = marca.len();
        (de..chars.len().saturating_sub(n - 1)).any(|j| chars[j..j + n] == *marca)
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`'
            && let Some(fim) = (i + 1..chars.len()).find(|&j| chars[j] == '`')
        {
            empurrar(&mut saida, &mut atual, negrito, italico, false);
            let codigo: String = chars[i + 1..fim].iter().collect();
            if !codigo.is_empty() {
                saida.push(Trecho { texto: codigo, negrito: false, italico: false, codigo: true });
            }
            i = fim + 1;
            continue;
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if negrito || fecha(i + 2, &['*', '*']) {
                empurrar(&mut saida, &mut atual, negrito, italico, false);
                negrito = !negrito;
            } else {
                atual.push_str("**");
            }
            i += 2;
            continue;
        }
        if c == '*' && (italico || (chars.get(i + 1).is_some_and(|p| !p.is_whitespace()) && fecha(i + 1, &['*']))) {
            empurrar(&mut saida, &mut atual, negrito, italico, false);
            italico = !italico;
            i += 1;
            continue;
        }
        atual.push(c);
        i += 1;
    }
    empurrar(&mut saida, &mut atual, negrito, italico, false);
    saida
}

/// O texto sem marcação (para medir e para a dica).
pub fn texto_simples(trechos: &[Trecho]) -> String {
    trechos.iter().map(|t| t.texto.as_str()).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(texto: &str) -> Trecho {
        Trecho { texto: texto.into(), ..Default::default() }
    }

    #[test]
    fn titulos_paragrafos_e_espaco() {
        let b = ler("# Fluxo da tela\n## Passos\n### Detalhe\nprimeira linha\nsegunda linha\n\n\nfim\n\n");
        assert_eq!(b[0], Bloco::Titulo(1, vec![t("Fluxo da tela")]));
        assert_eq!(b[1], Bloco::Titulo(2, vec![t("Passos")]));
        assert_eq!(b[2], Bloco::Titulo(3, vec![t("Detalhe")]));
        assert_eq!(b[3], Bloco::Paragrafo(vec![vec![t("primeira linha")], vec![t("segunda linha")]]));
        assert_eq!(b[4], Bloco::Vazio);
        assert_eq!(b[5], Bloco::Paragrafo(vec![vec![t("fim")]]));
        assert_eq!(b.len(), 6);
        // "#sem espaço" e "####" não são títulos.
        assert!(matches!(&ler("#tag")[0], Bloco::Paragrafo(_)));
        assert!(matches!(&ler("#### quatro")[0], Bloco::Paragrafo(_)));
    }

    #[test]
    fn enfase_e_codigo_na_linha() {
        let r = trechos("um **forte** e *leve* com `x = 1`");
        assert_eq!(
            r,
            vec![
                t("um "),
                Trecho { texto: "forte".into(), negrito: true, ..Default::default() },
                t(" e "),
                Trecho { texto: "leve".into(), italico: true, ..Default::default() },
                t(" com "),
                Trecho { texto: "x = 1".into(), codigo: true, ..Default::default() },
            ]
        );
        // Marcadores sem par ficam como texto.
        assert_eq!(texto_simples(&trechos("2 * 3 = 6 e **sem fim")), "2 * 3 = 6 e **sem fim");
        assert_eq!(texto_simples(&trechos("crase `solta")), "crase `solta");
        assert_eq!(trechos("**tudo forte**"), vec![Trecho { texto: "tudo forte".into(), negrito: true, ..Default::default() }]);
    }

    #[test]
    fn listas_numeros_e_caixas() {
        let b = ler("- um\n  - dentro\n1. primeiro\n2) segundo\n- [ ] fazer\n- [x] feito\n* outro");
        assert_eq!(b[0], Bloco::Item { nivel: 0, marcador: Marcador::Ponto, trechos: vec![t("um")] });
        assert_eq!(b[1], Bloco::Item { nivel: 1, marcador: Marcador::Ponto, trechos: vec![t("dentro")] });
        assert_eq!(b[2], Bloco::Item { nivel: 0, marcador: Marcador::Numero(1), trechos: vec![t("primeiro")] });
        assert_eq!(b[3], Bloco::Item { nivel: 0, marcador: Marcador::Numero(2), trechos: vec![t("segundo")] });
        assert_eq!(b[4], Bloco::Item { nivel: 0, marcador: Marcador::Caixa(false), trechos: vec![t("fazer")] });
        assert_eq!(b[5], Bloco::Item { nivel: 0, marcador: Marcador::Caixa(true), trechos: vec![t("feito")] });
        assert!(matches!(b[6], Bloco::Item { marcador: Marcador::Ponto, .. }));
    }

    #[test]
    fn tabela_com_alinhamento() {
        let b = ler("| Etapa | Tempo | Ok |\n|:--|:--:|--:|\n| **tela** | 2 h | sim |\n| api | 1 h |");
        let Bloco::Tabela { alinhamentos, cabecalho, linhas } = &b[0] else { panic!("{b:?}") };
        assert_eq!(alinhamentos, &vec![Alinhamento::Esquerda, Alinhamento::Centro, Alinhamento::Direita]);
        assert_eq!(cabecalho.as_ref().unwrap()[1], vec![t("Tempo")]);
        assert_eq!(linhas.len(), 2);
        assert!(linhas[0][0][0].negrito);
        assert_eq!(linhas[1].len(), 2);
        // Sem separador: tabela sem cabeçalho.
        let Bloco::Tabela { cabecalho, linhas, .. } = &ler("| a | b |\n| c | d |")[0] else { panic!() };
        assert!(cabecalho.is_none() && linhas.len() == 2);
    }

    #[test]
    fn bloco_de_codigo_guarda_o_texto_cru() {
        let b = ler("antes\n```\ntela -> api\n  **não é negrito**\n```\ndepois");
        assert_eq!(b[1], Bloco::Codigo("tela -> api\n  **não é negrito**".into()));
        assert_eq!(b[2], Bloco::Paragrafo(vec![vec![t("depois")]]));
        // Sem fechar: vai até o fim.
        assert_eq!(ler("```rust\nfn x()")[0], Bloco::Codigo("fn x()".into()));
    }

    #[test]
    fn texto_malformado_nao_quebra() {
        for texto in ["", "\n\n", "|", "| |\n|--", "- ", "1.", "```", "# ", "**", "*", "`", "|:-:|\n|x|", "- [", "#####"] {
            let _ = ler(texto);
        }
        assert!(ler("").is_empty());
    }
}
