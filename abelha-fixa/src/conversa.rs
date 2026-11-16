//! As linhas do terminal do agente viram blocos de conversa: o que você
//! mandou, as frases do agente, as chamadas de ferramenta (agrupadas, com o
//! resultado) e a caixa de pergunta com as opções. Segue o jeito do Claude
//! Code de escrever, no formato antigo ("⏺ Read(arquivo)", "⎿ resultado", a
//! caixa com bordas) e no atual ("❯ sua mensagem", "Read 1 file" debaixo da
//! frase, o menu com descrição em cada opção); o que não se encaixa vira
//! nota ou frase, então outros agentes ainda se leem.

#[derive(Clone, Debug, PartialEq)]
pub enum Bloco {
    /// O que você mandou (volta como "> texto" ou "❯ texto").
    Seu(String),
    /// Uma frase do agente.
    Frase(String),
    /// Uma linha solta do programa (compactou, retomou a sessão…).
    Nota(String),
    /// Chamadas de ferramenta seguidas.
    Ferramentas(Vec<Ferramenta>),
    /// A caixa em que o agente pede algo (aprovação, uma escolha).
    Pergunta(Pergunta),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ferramenta {
    /// O nome da ferramenta ("Read") ou, no formato atual, o resumo já
    /// traduzido ("Leu 1 arquivo"), sem argumento.
    pub nome: String,
    pub argumento: String,
    /// As primeiras linhas do resultado, sem o marcador.
    pub resultado: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pergunta {
    /// "Bash command", "Edit file", o título do menu; vazio num menu solto.
    pub titulo: String,
    /// O comando ou o arquivo, como veio.
    pub comando: Vec<String>,
    /// O resto do texto, inclusive a pergunta ("Do you want to proceed?").
    pub texto: Vec<String>,
    pub opcoes: Vec<(u8, String)>,
    /// Um menu fora da caixa: as linhas recuadas são a descrição das opções.
    menu: bool,
}

const MAX_RESULTADO: usize = 6;

impl Ferramenta {
    /// "Leu Cupom.tsx", "Rodou npm test", "Editou Checkout.tsx".
    pub fn resumo(&self) -> String {
        let verbo = match self.nome.as_str() {
            "Read" => "Leu",
            "Write" => "Escreveu",
            "Edit" | "Update" | "MultiEdit" => "Editou",
            "Bash" => "Rodou",
            "Grep" | "Glob" | "Search" | "WebSearch" => "Procurou",
            "Task" => "Delegou",
            "WebFetch" | "Fetch" => "Abriu",
            "TodoWrite" => "Anotou",
            outro => outro,
        };
        let alvo = self.alvo();
        if alvo.is_empty() { verbo.to_string() } else { format!("{verbo} {alvo}") }
    }

    /// O argumento encurtado: o nome do arquivo num caminho, o comando como veio.
    pub fn alvo(&self) -> String {
        let a = self.argumento.trim().trim_matches('"');
        let curto = match self.nome.as_str() {
            "Bash" | "Grep" | "Glob" | "Search" | "WebSearch" | "Task" | "WebFetch" | "Fetch" => a.to_string(),
            _ => a.rsplit('/').next().unwrap_or(a).to_string(),
        };
        let mut s: String = curto.chars().take(48).collect();
        if curto.chars().count() > 48 {
            s.push('…');
        }
        s
    }

    /// "+1 −2" de uma edição, quando o resultado diz.
    pub fn contagem(&self) -> Option<(u32, u32)> {
        let r = self.resultado.first()?;
        let numero_antes = |marca: &str| -> Option<u32> {
            let i = r.find(marca)?;
            r[..i].split_whitespace().last()?.parse().ok()
        };
        Some((numero_antes(" addition")?, numero_antes(" removal")?))
    }

    /// Se há algo a mostrar além do resumo.
    pub fn tem_detalhe(&self) -> bool {
        !self.argumento.is_empty() || !self.resultado.is_empty()
    }
}

impl Pergunta {
    /// Se é um pedido de aprovação (um comando, uma edição) ou uma pergunta.
    pub fn aprovacao(&self) -> bool {
        let t = self.titulo.to_lowercase();
        t.contains("command") || t.contains("edit") || t.contains("write") || t.contains("permission") || t.contains("tool")
    }
}

/// "1. Yes", "❯ 2. No" vira (2, "No").
fn opcao(linha: &str) -> Option<(u8, String)> {
    let t = linha.trim_start_matches(['❯', '>', ' ']).trim();
    let (n, resto) = t.split_once(". ")?;
    if n.len() != 1 || resto.trim().is_empty() {
        return None;
    }
    let n = n.parse::<u8>().ok().filter(|n| (1..=9).contains(n))?;
    Some((n, traduzir_opcao(resto.trim())))
}

/// As opções fixas do Claude Code em português; as outras ficam como vieram.
fn traduzir_opcao(texto: &str) -> String {
    match texto.trim_end_matches('.') {
        "Yes" => "Sim",
        "No" => "Não",
        "Yes, and don't ask again this session" | "Yes, and don't ask again" => "Sim, sempre nesta sessão",
        "No, and tell Claude what to do differently" | "No, and tell Claude what to do" => "Não, dizer o que fazer",
        "Type something" => "Escrever outra coisa",
        "Chat about this" => "Conversar sobre isso",
        _ => return texto.to_string(),
    }
    .to_string()
}

/// "⏺ Read(src/a.ts)" vira ("Read", "src/a.ts"); uma frase não.
fn chamada(texto: &str) -> Option<(String, String)> {
    let (nome, resto) = texto.split_once('(')?;
    if nome.is_empty() || !nome.chars().next().is_some_and(|c| c.is_ascii_uppercase()) || !nome.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((nome.to_string(), resto.trim_end_matches(')').to_string()))
}

/// O resumo que o Claude Code atual escreve debaixo da frase ("Read 1 file",
/// "Listed 1 directory, called colmeia, ran 3 shell commands") traduzido, uma
/// ferramenta por trecho. Nada se algum trecho não começar com um verbo dele.
fn acoes(texto: &str) -> Option<Vec<Ferramenta>> {
    let texto = texto.split(" · ").next().unwrap_or(texto).trim_end_matches('…');
    let mut lista = Vec::new();
    for trecho in texto.split(", ") {
        let trecho = trecho.trim();
        let (verbo, resto) = trecho.split_once(' ').unwrap_or((trecho, ""));
        let verbo = match verbo.to_ascii_lowercase().as_str() {
            "read" | "reading" => "Leu",
            "ran" | "running" => "Rodou",
            "searched" | "searching" | "grepped" | "explored" | "exploring" => "Procurou",
            "edited" | "editing" | "updated" | "updating" => "Editou",
            "wrote" | "writing" | "created" | "creating" => "Escreveu",
            "called" | "calling" => "Chamou",
            "listed" | "listing" => "Listou",
            "fetched" | "fetching" => "Abriu",
            "deleted" | "deleting" | "removed" => "Apagou",
            _ => return None,
        };
        let resto = resto
            .replace(" shell commands", " comandos")
            .replace(" shell command", " comando")
            .replace(" directories", " pastas")
            .replace(" directory", " pasta")
            .replace(" files", " arquivos")
            .replace(" file", " arquivo")
            .replace(" times", " vezes")
            .replace(" time", " vez")
            .replace(" lines", " linhas");
        let nome = if resto.is_empty() { verbo.to_string() } else { format!("{verbo} {resto}") };
        lista.push(Ferramenta { nome, argumento: String::new(), resultado: Vec::new() });
    }
    (!lista.is_empty()).then_some(lista)
}

const MARCADORES: [char; 2] = ['⏺', '●'];
const RESULTADOS: [char; 2] = ['⎿', '└'];
/// Os glifos de espera do Claude Code na linha de estado ("✻ Cooked for 3m").
const ESPERA: [char; 5] = ['✻', '✽', '✶', '✢', '·'];

/// O que não é conversa: as linhas só de traços ou do logo, a linha de
/// estado e os rodapés de menu.
const LOGO: &str = "▐▛▜▝▘▖▗█▀▄▌▍▎▏ ";

fn descartar(t: &str) -> bool {
    t.chars().all(|c| "─━═│┃".contains(c) || LOGO.contains(c))
        || t.starts_with("⏵⏵")
        || t.contains("Enter to select")
        || t.contains("Esc to cancel")
        || t.contains("shift+tab to cycle")
        || t.contains("ctrl+g to edit")
}

/// Tira a caixa de digitar e o que vem depois dela (a linha de estado): no
/// Claude Code atual ela começa com "❯" e um espaço duro, diferente da sua
/// mensagem já enviada, que vem com espaço comum.
fn sem_a_caixa(linhas: &[String]) -> &[String] {
    let caixa = linhas.iter().rposition(|l| {
        let t = l.trim_start();
        t.starts_with("❯\u{a0}") || t.starts_with(">\u{a0}") || t == "❯"
    });
    match caixa {
        Some(i) => &linhas[..i],
        None => linhas,
    }
}

/// O marcador e o resto de um item de lista ("- a", "* a", "• a", "1. a");
/// None numa linha comum.
pub fn marcador_de_lista(t: &str) -> Option<(&str, &str)> {
    for m in ["- ", "* ", "• ", "– "] {
        if let Some(resto) = t.strip_prefix(m) {
            return Some(("•", resto.trim_start()));
        }
    }
    let (n, resto) = t.split_once(". ")?;
    ((1..=2).contains(&n.len()) && n.bytes().all(|c| c.is_ascii_digit())).then(|| (&t[..n.len() + 1], resto.trim_start()))
}

fn item_de_lista(t: &str) -> bool {
    marcador_de_lista(t).is_some()
}

/// Uma linha de tabela, como o Claude Code desenha (┌─┬─┐, │ a │ b │,
/// ├─┼─┤, └─┴─┘) ou como o Markdown escreve (| a | b |, |---|---|). Na
/// frase ela fica normalizada: a linha de conteúdo vira "│a│b│" (as células
/// entre barras, sem os espaços), a divisão entre linhas vira "├" e as
/// bordas de cima e de baixo viram vazio (não entram). None numa linha comum.
pub fn linha_de_tabela(t: &str) -> Option<String> {
    let t = t.trim();
    let primeiro = t.chars().next()?;
    if "┌└┏┗╔╚".contains(primeiro) {
        return Some(String::new());
    }
    if "├┣╠╞".contains(primeiro) {
        return Some("├".into());
    }
    // A divisão do Markdown: "|---|:--:|".
    if primeiro == '|' && t.contains('-') && t.chars().all(|c| "|-: ".contains(c)) {
        return Some("├".into());
    }
    let barra = match primeiro {
        '│' | '┃' | '|' => primeiro,
        _ => return None,
    };
    let sem_barras = t.strip_suffix(barra)?.get(barra.len_utf8()..)?;
    let celulas: Vec<&str> = sem_barras.split(barra).map(str::trim).collect();
    if celulas.is_empty() || celulas.iter().all(|c| c.is_empty()) {
        return None;
    }
    Some(format!("│{}│", celulas.join("│")))
}

/// As linhas de uma tabela da frase (as de "│a│b│" e as divisões "├") em
/// linhas de células, a primeira sendo o cabeçalho. Quando a tabela tem
/// divisão entre cada linha (o jeito do Claude Code) e uma linha comprida
/// quebrou em duas no terminal, as duas viram uma só.
pub fn tabela(linhas: &[&str]) -> Vec<Vec<String>> {
    let mut grupos: Vec<Vec<Vec<String>>> = vec![Vec::new()];
    for l in linhas {
        if *l == "├" {
            if !grupos.last().is_some_and(Vec::is_empty) {
                grupos.push(Vec::new());
            }
            continue;
        }
        let Some(miolo) = l.strip_prefix('│').and_then(|r| r.strip_suffix('│')) else { continue };
        grupos.last_mut().unwrap().push(miolo.split('│').map(str::to_string).collect());
    }
    grupos.retain(|g| !g.is_empty());
    let juntar = grupos.len() >= 3 && grupos.iter().any(|g| g.len() > 1);
    let mut saida: Vec<Vec<String>> = Vec::new();
    for g in grupos {
        if !juntar {
            saida.extend(g);
            continue;
        }
        let colunas = g.iter().map(Vec::len).max().unwrap_or(0);
        let mut linha = vec![String::new(); colunas];
        for l in g {
            for (i, c) in l.iter().enumerate() {
                if !c.is_empty() {
                    if !linha[i].is_empty() {
                        linha[i].push(' ');
                    }
                    linha[i].push_str(c);
                }
            }
        }
        saida.push(linha);
    }
    let colunas = saida.iter().map(Vec::len).max().unwrap_or(0);
    for l in &mut saida {
        l.resize(colunas, String::new());
    }
    saida
}

/// Acrescenta a uma frase a linha que veio depois: uma linha de tabela na
/// sua linha; a comum, no parágrafo (nova depois de uma linha vazia, de um
/// item de lista ou de uma tabela).
fn acrescentar(f: &mut String, t: &str, tabela: Option<String>, apos_vazia: bool) {
    if let Some(l) = tabela {
        if !l.is_empty() {
            if !f.is_empty() {
                f.push('\n');
            }
            f.push_str(&l);
        }
        return;
    }
    let depois_de_tabela = f.ends_with('│') || f.ends_with('├');
    if apos_vazia && !depois_de_tabela {
        f.push_str("\n\n");
    } else if item_de_lista(t) || depois_de_tabela {
        f.push('\n');
    } else {
        f.push(' ');
    }
    f.push_str(t);
}

/// Tira o aviso que o Claude Code sobrepõe ao texto quando a tela está
/// rolada ("Jump to bottom (ctrl+End) ↓"), que não é parte da conversa.
fn sem_sobreposicao(linha: &str) -> std::borrow::Cow<'_, str> {
    const AVISO: &str = "Jump to bottom";
    let Some(i) = linha.find(AVISO) else { return std::borrow::Cow::Borrowed(linha) };
    let depois = &linha[i..];
    let fim = depois.find('↓').map(|j| j + '↓'.len_utf8()).or_else(|| depois.find(')').map(|j| j + 1)).unwrap_or(depois.len());
    std::borrow::Cow::Owned(format!("{} {}", linha[..i].trim_end(), depois[fim..].trim_start()))
}

pub fn interpretar(linhas: &[String]) -> Vec<Bloco> {
    let mut blocos: Vec<Bloco> = Vec::new();
    let mut na_caixa = false;
    // Houve uma linha em branco desde o último bloco: o parágrafo seguinte
    // fica separado dentro da frase.
    let mut pulou = false;
    for linha in sem_a_caixa(linhas) {
        let limpa = sem_sobreposicao(linha);
        let l = limpa.trim_end();
        // O logo do Claude Code fica à esquerda das primeiras linhas.
        let t = l.trim_start().trim_start_matches(|c: char| LOGO.contains(c));
        // A caixa da pergunta, com as bordas.
        if t.starts_with('╭') {
            na_caixa = true;
            blocos.push(Bloco::Pergunta(Pergunta::default()));
            continue;
        }
        if t.starts_with('╰') {
            na_caixa = false;
            continue;
        }
        if na_caixa {
            let conteudo = t.trim_start_matches('│').trim_end_matches('│').trim();
            if conteudo.is_empty() {
                continue;
            }
            if let Some(Bloco::Pergunta(p)) = blocos.last_mut() {
                if let Some(o) = opcao(conteudo) {
                    p.opcoes.push(o);
                } else if p.titulo.is_empty() {
                    p.titulo = conteudo.to_string();
                } else if p.comando.is_empty() && !conteudo.ends_with('?') {
                    p.comando.push(conteudo.to_string());
                } else {
                    p.texto.push(conteudo.to_string());
                }
            }
            continue;
        }
        if t.is_empty() {
            pulou = true;
            continue;
        }
        if descartar(t) {
            continue;
        }
        let apos_vazia = std::mem::take(&mut pulou);
        // Uma opção numerada solta (o menu do agente fora da caixa).
        if let Some(o) = opcao(t)
            && (t.starts_with('❯') || matches!(blocos.last(), Some(Bloco::Pergunta(_))) || matches!(blocos.last(), Some(Bloco::Frase(f)) if f.ends_with('?')))
        {
            match blocos.last_mut() {
                Some(Bloco::Pergunta(p)) => p.opcoes.push(o),
                _ => {
                    // O que vem logo antes é a pergunta: a frase do agente,
                    // o título do menu ("☐ Rodar ls -la") e o texto dele.
                    let mut p = Pergunta { opcoes: vec![o], menu: true, ..Pergunta::default() };
                    let do_menu = |s: &str| s.starts_with(['☐', '☑', '☒']) || s.ends_with('?');
                    while p.texto.len() + usize::from(!p.titulo.is_empty()) < 3 {
                        let s = match blocos.last() {
                            Some(Bloco::Frase(s)) if p.texto.is_empty() => s.clone(),
                            Some(Bloco::Nota(s)) if do_menu(s) => s.clone(),
                            _ => break,
                        };
                        blocos.pop();
                        if let Some(titulo) = s.strip_prefix(['☐', '☑', '☒']) {
                            p.titulo = titulo.trim().to_string();
                        } else {
                            p.texto.insert(0, s);
                        }
                    }
                    blocos.push(Bloco::Pergunta(p));
                }
            }
            continue;
        }
        if let Some(resto) = t.strip_prefix("> ").or_else(|| t.strip_prefix("❯ ")) {
            let resto = resto.trim();
            blocos.push(Bloco::Seu(linha_de_tabela(resto).unwrap_or_else(|| resto.to_string())));
            continue;
        }
        if let Some(resto) = t.strip_prefix(MARCADORES) {
            let resto = resto.trim();
            match chamada(resto) {
                Some((nome, argumento)) => {
                    let f = Ferramenta { nome, argumento, resultado: Vec::new() };
                    match blocos.last_mut() {
                        Some(Bloco::Ferramentas(v)) => v.push(f),
                        _ => blocos.push(Bloco::Ferramentas(vec![f])),
                    }
                }
                None => blocos.push(Bloco::Frase(linha_de_tabela(resto).unwrap_or_else(|| resto.to_string()))),
            }
            continue;
        }
        // Uma tabela (do agente ou colada por você) continua a frase, mesmo
        // sem recuo; a borda de baixo "└" não é o resultado de ferramenta.
        let tabela = linha_de_tabela(t);
        if tabela.is_some()
            && let Some(Bloco::Frase(f)) | Some(Bloco::Seu(f)) = blocos.last_mut()
        {
            acrescentar(f, t, tabela, false);
            continue;
        }
        if let Some(resto) = t.strip_prefix(RESULTADOS) {
            if let Some(Bloco::Ferramentas(v)) = blocos.last_mut()
                && let Some(f) = v.last_mut()
                && f.resultado.len() < MAX_RESULTADO
            {
                f.resultado.push(resto.trim().to_string());
            }
            continue;
        }
        if let Some(resto) = t.strip_prefix(ESPERA)
            && resto.starts_with(' ')
        {
            blocos.push(Bloco::Nota(resto.trim().to_string()));
            continue;
        }
        let recuada = l.starts_with(' ');
        // O resumo das ferramentas debaixo da frase, no formato atual.
        if recuada
            && let Some(lista) = acoes(t)
            && !matches!(blocos.last(), Some(Bloco::Pergunta(_)))
        {
            match blocos.last_mut() {
                Some(Bloco::Ferramentas(v)) => v.extend(lista),
                _ => blocos.push(Bloco::Ferramentas(lista)),
            }
            continue;
        }
        // Uma linha recuada continua o bloco de antes: o resultado da
        // ferramenta ou a frase quebrada.
        match blocos.last_mut() {
            Some(Bloco::Ferramentas(v)) if recuada => {
                if let Some(f) = v.last_mut()
                    && f.resultado.len() < MAX_RESULTADO
                {
                    f.resultado.push(t.to_string());
                }
            }
            Some(Bloco::Frase(f)) | Some(Bloco::Seu(f)) if recuada => {
                // A frase guarda a estrutura: parágrafos separados por uma
                // linha vazia, cada item de lista e cada linha de tabela na
                // sua linha.
                acrescentar(f, t, tabela, apos_vazia);
            }
            Some(Bloco::Pergunta(p)) if recuada => {
                if !p.menu {
                    p.texto.push(t.to_string());
                }
            }
            _ => match tabela {
                Some(l) if !l.is_empty() => blocos.push(Bloco::Frase(l)),
                Some(_) => {}
                None => blocos.push(Bloco::Nota(t.to_string())),
            },
        }
    }
    // Só vale uma lista de opções que começa no 1 e segue sem buracos. Uma
    // caixa sem opções e sem pergunta (a de boas-vindas) é só uma nota.
    for b in &mut blocos {
        if let Bloco::Pergunta(p) = b {
            p.opcoes.sort_by_key(|(n, _)| *n);
            p.opcoes.dedup_by_key(|(n, _)| *n);
            if p.opcoes.first().is_none_or(|(n, _)| *n != 1) || p.opcoes.iter().enumerate().any(|(i, (n, _))| *n as usize != i + 1) {
                p.opcoes.clear();
            }
            p.titulo = p.titulo.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
            if p.opcoes.is_empty() && !p.texto.iter().chain(p.comando.iter()).any(|t| t.ends_with('?')) {
                *b = Bloco::Nota(p.titulo.clone());
            }
        }
    }
    blocos
}

/// As opções da última pergunta, para os botões.
pub fn opcoes(blocos: &[Bloco]) -> Vec<(u8, String)> {
    match blocos.last() {
        Some(Bloco::Pergunta(p)) => p.opcoes.clone(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn linhas(texto: &str) -> Vec<String> {
        texto.lines().map(str::to_string).collect()
    }

    #[test]
    fn claude_code_vira_blocos() {
        let b = interpretar(&linhas(
            "> O desconto está sendo aplicado duas vezes\n\n⏺ Vou procurar onde o cupom é aplicado.\n\n⏺ Read(src/components/Cupom.tsx)\n  ⎿  Read 7 lines\n\n⏺ Bash(grep -rn \"aplicarCupom\" src)\n  ⎿  src/a.ts:1\n     src/b.ts:42\n\n⏺ Achei: na linha 42 e de novo na 58,\n  depois do frete.\n\n⏺ Update(src/pages/Checkout.tsx)\n  ⎿  Updated src/pages/Checkout.tsx with 1 additions and 2 removals\n\n╭──────────────╮\n│ Bash command │\n│              │\n│   npm test -- Checkout │\n│   Roda os testes       │\n│              │\n│ Do you want to proceed? │\n│ ❯ 1. Yes     │\n│   2. Yes, and don't ask again │\n│   3. No      │\n╰──────────────╯",
        ));
        assert_eq!(b[0], Bloco::Seu("O desconto está sendo aplicado duas vezes".into()));
        assert_eq!(b[1], Bloco::Frase("Vou procurar onde o cupom é aplicado.".into()));
        let Bloco::Ferramentas(f) = &b[2] else { panic!("{:?}", b[2]) };
        assert_eq!(f.iter().map(Ferramenta::resumo).collect::<Vec<_>>(), vec!["Leu Cupom.tsx", "Rodou grep -rn \"aplicarCupom\" src"]);
        assert_eq!(f[1].resultado, vec!["src/a.ts:1", "src/b.ts:42"]);
        assert_eq!(b[3], Bloco::Frase("Achei: na linha 42 e de novo na 58, depois do frete.".into()));
        let Bloco::Ferramentas(f) = &b[4] else { panic!() };
        assert_eq!((f[0].resumo(), f[0].contagem()), ("Editou Checkout.tsx".to_string(), Some((1, 2))));
        let Bloco::Pergunta(p) = &b[5] else { panic!("{:?}", b[5]) };
        assert!(p.aprovacao());
        assert_eq!(
            (p.titulo.as_str(), p.comando.as_slice(), p.texto.as_slice()),
            ("Bash command", &["npm test -- Checkout".to_string()][..], &["Roda os testes".to_string(), "Do you want to proceed?".to_string()][..])
        );
        assert_eq!(opcoes(&b).len(), 3);
        assert_eq!(opcoes(&b)[1].1, "Sim, sempre nesta sessão");
    }

    #[test]
    fn menu_solto_e_notas_e_outros_agentes() {
        let b = interpretar(&linhas("Compacted while idle\n⏺ Quer que eu aplique no carrinho também?\n❯ 1. Yes\n  2. No\ntexto solto de outro agente"));
        assert_eq!(b[0], Bloco::Nota("Compacted while idle".into()));
        let Bloco::Pergunta(p) = &b[1] else { panic!("{:?}", b[1]) };
        assert!(!p.aprovacao());
        assert_eq!(p.texto, vec!["Quer que eu aplique no carrinho também?"]);
        assert_eq!(p.opcoes, vec![(1, "Sim".to_string()), (2, "Não".to_string())]);
        assert_eq!(b[2], Bloco::Nota("texto solto de outro agente".into()));
        // Uma lista que não começa no 1 não vira botão.
        let b = interpretar(&linhas("⏺ Escolha\n❯ 2. B\n  3. C"));
        assert!(opcoes(&b).is_empty());
        // A caixa de boas-vindas não é pergunta.
        let b = interpretar(&linhas("╭────╮\n│ ✻ Welcome to Claude Code! │\n│ /help for help │\n│ cwd: ~/projetos │\n╰────╯"));
        assert_eq!(b, vec![Bloco::Nota("Welcome to Claude Code!".into())]);
    }

    /// A tela do Claude Code 2.1, como o núcleo entrega: o logo, a sua
    /// mensagem com "❯", o resumo das ferramentas debaixo da frase, o menu
    /// com descrição em cada opção e a caixa de digitar no fim.
    #[test]
    fn claude_code_atual() {
        let b = interpretar(&linhas(
            "\n ▐▛███▛█   Claude Code v2.1.310\n▝▜██████▀  Fable 5.1 with high effort · Claude Team\n ▝▝   ▝▝   /home/eu/projetos/loja-web\n\n❯ Leia o README.md e me diga em uma frase do que ele trata. Depois pergunte se eu quero que você rode o\n  comando ls -la.\n\n● Vou ler o README.md desta pasta.\n\n  Read 1 file\n\n● O README.md diz que o projeto é o front-end de uma loja de exemplo.\n\n  Listed 1 directory, called colmeia, ran 3 shell commands\n\n✻ Cooked for 3m 44s · done 15:46\n────────────────────────────────\n ☐ Rodar ls -la\n\nQuer que eu rode o comando `ls -la` nesta pasta?\n\n❯ 1. Sim, pode rodar\n     Executo o comando com a ferramenta Bash e mostro a listagem.\n  2. Não, por enquanto não\n     Não executo nada agora.\n  3. Type something.\n────────────────────────────────\n  4. Chat about this\n\nEnter to select · ↑/↓ to navigate · Esc to cancel\n────────────────────────────────\n❯\u{a0}\n────────────────────────────────\n  Colmeia\n  ⏵⏵ auto mode on (shift+tab to cycle)",
        ));
        assert_eq!(b[0], Bloco::Nota("Claude Code v2.1.310".into()));
        assert_eq!(b[1], Bloco::Nota("Fable 5.1 with high effort · Claude Team".into()));
        assert_eq!(b[2], Bloco::Nota("/home/eu/projetos/loja-web".into()));
        assert_eq!(
            b[3],
            Bloco::Seu("Leia o README.md e me diga em uma frase do que ele trata. Depois pergunte se eu quero que você rode o comando ls -la.".into())
        );
        assert_eq!(b[4], Bloco::Frase("Vou ler o README.md desta pasta.".into()));
        let Bloco::Ferramentas(f) = &b[5] else { panic!("{:?}", b[5]) };
        assert_eq!(f[0].resumo(), "Leu 1 arquivo");
        assert!(!f[0].tem_detalhe());
        assert_eq!(b[6], Bloco::Frase("O README.md diz que o projeto é o front-end de uma loja de exemplo.".into()));
        let Bloco::Ferramentas(f) = &b[7] else { panic!("{:?}", b[7]) };
        assert_eq!(f.iter().map(Ferramenta::resumo).collect::<Vec<_>>(), vec!["Listou 1 pasta", "Chamou colmeia", "Rodou 3 comandos"]);
        assert_eq!(b[8], Bloco::Nota("Cooked for 3m 44s · done 15:46".into()));
        let Bloco::Pergunta(p) = &b[9] else { panic!("{:?}", b[9]) };
        assert!(!p.aprovacao());
        assert_eq!(p.titulo, "Rodar ls -la");
        assert_eq!(p.texto, vec!["Quer que eu rode o comando `ls -la` nesta pasta?"]);
        assert_eq!(
            p.opcoes,
            vec![
                (1, "Sim, pode rodar".to_string()),
                (2, "Não, por enquanto não".to_string()),
                (3, "Escrever outra coisa".to_string()),
                (4, "Conversar sobre isso".to_string())
            ]
        );
        assert_eq!(b.len(), 10, "{b:?}");
    }

    #[test]
    fn lista_numerada_depois_de_uma_pergunta_vira_opcoes() {
        let b = interpretar(&linhas("● Como você prefere que eu comece?\n  1. Ler o README do projeto.\n  2. Listar a pasta.\n\n✻ Worked for 3s"));
        let Bloco::Pergunta(p) = &b[0] else { panic!("{:?}", b[0]) };
        assert_eq!(p.texto, vec!["Como você prefere que eu comece?"]);
        assert_eq!(p.opcoes, vec![(1, "Ler o README do projeto.".to_string()), (2, "Listar a pasta.".to_string())]);
        assert_eq!(b[1], Bloco::Nota("Worked for 3s".into()));
    }

    #[test]
    fn paragrafos_e_itens_ficam_separados_na_frase() {
        let b = interpretar(&linhas(
            "● Rotina criada.\n\n  O que foi feito\n\n  - O projeto entrou como git.\n  - A rotina foi criada, com\n    duas fontes.\n\n  Para a rotina escrever Jump to bottom (ctrl+End) ↓ junto com o título.",
        ));
        assert_eq!(
            b,
            vec![Bloco::Frase("Rotina criada.\n\nO que foi feito\n\n- O projeto entrou como git.\n- A rotina foi criada, com duas fontes.\n\nPara a rotina escrever junto com o título.".into())]
        );
        assert_eq!(marcador_de_lista("- a"), Some(("•", "a")));
        assert_eq!(marcador_de_lista("12. a"), Some(("12.", "a")));
        assert_eq!(marcador_de_lista("2024. a"), None);
        assert_eq!(marcador_de_lista("texto"), None);
    }

    #[test]
    fn tabela_do_terminal_fica_em_linhas_na_frase() {
        let b = interpretar(&linhas(
            "❯ ┌──────────┬───────────┐\n  │ O quê    │ Nome      │\n  ├──────────┼───────────┤\n  │ Variável │ ENDERECO_ │\n  │          │ GESTAO    │\n  ├──────────┼───────────┤\n  │ Flag     │ -gestao   │\n  └──────────┴───────────┘\n  ; 1 - isso seria o DNS?\n\n● Sim, é o DNS:\n\n  | a | b |\n  |---|---|\n  | 1 | 2 |\n  | 3 | 4 |\n\n  Pronto.",
        ));
        assert_eq!(b[0], Bloco::Seu("│O quê│Nome│\n├\n│Variável│ENDERECO_│\n││GESTAO│\n├\n│Flag│-gestao│\n; 1 - isso seria o DNS?".into()));
        assert_eq!(b[1], Bloco::Frase("Sim, é o DNS:\n│a│b│\n├\n│1│2│\n│3│4│\nPronto.".into()));
        assert_eq!(b.len(), 2, "{b:?}");
        // Com divisão entre cada linha, a linha que quebrou no terminal junta de novo.
        assert_eq!(
            tabela(&["│O quê│Nome│", "├", "│Variável│ENDERECO_│", "││GESTAO│", "├", "│Flag│-gestao│"]),
            vec![vec!["O quê", "Nome"], vec!["Variável", "ENDERECO_ GESTAO"], vec!["Flag", "-gestao"]]
        );
        // Só com a divisão do cabeçalho, cada linha é uma linha.
        assert_eq!(tabela(&["│a│b│", "├", "│1│2│", "│3│4│"]), vec![vec!["a", "b"], vec!["1", "2"], vec!["3", "4"]]);
        assert_eq!(tabela(&["│a│b│c│", "│1│"]), vec![vec!["a", "b", "c"], vec!["1", "", ""]]);
        assert_eq!(linha_de_tabela("│ só texto"), None);
        assert_eq!(linha_de_tabela("texto │ com barra │"), None);
        assert_eq!(linha_de_tabela("| x |").as_deref(), Some("│x│"));
    }

    #[test]
    fn caixa_de_digitar_com_texto_fica_de_fora() {
        let b = interpretar(&linhas("● Pronto.\n────\n❯\u{a0}oi, tudo bem\n────\n  Colmeia"));
        assert_eq!(b, vec![Bloco::Frase("Pronto.".into())]);
    }
}
