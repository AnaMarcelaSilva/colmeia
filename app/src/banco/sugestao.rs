//! O autocompletar do console: as palavras do SQL do dialeto e os nomes que
//! a árvore já carregou (esquemas, tabelas e colunas). Tudo local, sem IA e
//! sem consulta nova ao banco: as colunas que faltam são pedidas pela mesma
//! rota da árvore e ficam no cache dela. Desligado, nada daqui roda.

use std::collections::HashMap;

use super::sql::{self, Pedaco, Tipo};

/// Quantas sugestões o popup mostra.
pub const MAXIMO: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Especie {
    Coluna,
    Tabela,
    Esquema,
    Palavra,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sugestao {
    /// O que entra no editor (já citado quando o nome precisa).
    pub texto: String,
    pub especie: Especie,
    /// O tipo da coluna, o esquema da tabela…
    pub detalhe: String,
}

/// Os nomes já carregados de uma conexão (no banco do console).
#[derive(Default)]
pub struct Catalogo {
    pub esquemas: Vec<String>,
    /// (esquema, nome) das tabelas e views.
    pub tabelas: Vec<(String, String)>,
    /// Nome da tabela em minúsculas → (coluna, tipo).
    pub colunas: HashMap<String, Vec<(String, String)>>,
}

impl Catalogo {
    fn esquema(&self, nome: &str) -> bool {
        self.esquemas.iter().any(|e| e.eq_ignore_ascii_case(nome))
    }

    /// A tabela existe no que foi carregado (o esquema dela, para pedir as colunas).
    pub fn tabela(&self, nome: &str) -> Option<&(String, String)> {
        self.tabelas.iter().find(|(_, t)| t.eq_ignore_ascii_case(nome))
    }
}

/// Onde o cursor está, para escolher o que sugerir.
#[derive(Clone, Debug, PartialEq)]
pub enum Onde {
    /// Qualquer lugar: colunas das tabelas da instrução, tabelas e palavras.
    Geral,
    /// Depois de FROM, JOIN, INTO, UPDATE…: tabelas primeiro.
    Tabela,
    /// Depois de `nome.`: as tabelas do esquema ou as colunas da tabela
    /// (o apelido já trocado pelo nome da tabela).
    Depois(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Analise {
    /// Onde começa a palavra em edição (em bytes); ela vai até o cursor.
    pub inicio: usize,
    pub prefixo: String,
    pub onde: Onde,
    /// As tabelas citadas na instrução do cursor.
    pub tabelas: Vec<String>,
}

const DEPOIS_VEM_TABELA: [&str; 6] = ["FROM", "JOIN", "INTO", "UPDATE", "TABLE", "DESCRIBE"];

/// O que está sendo escrito no cursor (em bytes). `None` dentro de texto ou
/// comentário, ou fora de uma palavra (depois de um espaço).
pub fn analisar(dialeto: &str, texto: &str, cursor: usize) -> Option<Analise> {
    if cursor > texto.len() || !texto.is_char_boundary(cursor) {
        return None;
    }
    let pedacos: Vec<Pedaco> = sql::pedacos(dialeto, &texto[..cursor]).into_iter().filter(|p| p.tipo != Tipo::Comentario || p.fim == cursor).collect();
    let ultimo = pedacos.last()?;
    let trecho = |p: &Pedaco| &texto[p.ini..p.fim];
    // A palavra em edição (ou nada, logo depois de um ponto).
    let (inicio, antes) = match ultimo.tipo {
        Tipo::Palavra => (ultimo.ini, pedacos.len() - 1),
        Tipo::Simbolo if trecho(ultimo) == "." => (cursor, pedacos.len()),
        _ => return None,
    };
    let prefixo = texto[inicio..cursor].to_string();
    let instrucao = sql::sob_cursor(dialeto, texto, cursor).map_or("", |r| &texto[r.start..r.end.max(cursor).min(texto.len())]);
    let citadas = tabelas_citadas(dialeto, instrucao);
    // `nome.` antes da palavra.
    let qualificador = (antes >= 2 && trecho(&pedacos[antes - 1]) == "." && matches!(pedacos[antes - 2].tipo, Tipo::Palavra | Tipo::Citado))
        .then(|| sem_aspas(trecho(&pedacos[antes - 2])));
    let onde = match qualificador {
        Some(q) => {
            let tabela = citadas.iter().find(|(t, apelido)| apelido.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(&q)) || t.eq_ignore_ascii_case(&q));
            Onde::Depois(tabela.map_or(q, |(t, _)| t.clone()))
        }
        None => {
            let anterior = pedacos[..antes].iter().rev().find(|p| p.tipo != Tipo::Espaco && p.tipo != Tipo::Comentario);
            match anterior {
                Some(p) if p.tipo == Tipo::Palavra && DEPOIS_VEM_TABELA.iter().any(|k| k.eq_ignore_ascii_case(trecho(p))) => Onde::Tabela,
                _ => Onde::Geral,
            }
        }
    };
    let tabelas = citadas.into_iter().map(|(t, _)| t).collect();
    Some(Analise { inicio, prefixo, onde, tabelas })
}

/// As tabelas da instrução com o apelido de cada uma: `FROM pedidos p`,
/// `JOIN loja.clientes AS c`, `FROM a, b`. O esquema fica de fora.
pub fn tabelas_citadas(dialeto: &str, instrucao: &str) -> Vec<(String, Option<String>)> {
    let pedacos: Vec<Pedaco> = sql::pedacos(dialeto, instrucao).into_iter().filter(|p| p.tipo != Tipo::Espaco && p.tipo != Tipo::Comentario).collect();
    let trecho = |i: usize| pedacos.get(i).map_or("", |p| &instrucao[p.ini..p.fim]);
    let nome = |i: usize| pedacos.get(i).is_some_and(|p| matches!(p.tipo, Tipo::Palavra | Tipo::Citado) && !sql::palavra_chave(trecho(i)));
    let mut lista = Vec::new();
    let mut i = 0;
    while i < pedacos.len() {
        let chave = trecho(i);
        if !["FROM", "JOIN", "UPDATE", "INTO"].iter().any(|k| k.eq_ignore_ascii_case(chave)) {
            i += 1;
            continue;
        }
        i += 1;
        // Uma ou várias tabelas separadas por vírgula.
        while nome(i) {
            let mut tabela = sem_aspas(trecho(i));
            i += 1;
            while trecho(i) == "." && nome(i + 1) {
                tabela = sem_aspas(trecho(i + 1));
                i += 2;
            }
            if trecho(i).eq_ignore_ascii_case("AS") {
                i += 1;
            }
            let apelido = nome(i).then(|| sem_aspas(trecho(i)));
            if apelido.is_some() {
                i += 1;
            }
            lista.push((tabela, apelido));
            if trecho(i) != "," {
                break;
            }
            i += 1;
        }
    }
    lista
}

fn sem_aspas(nome: &str) -> String {
    let mut c = nome.chars();
    match (c.next(), nome.chars().last()) {
        (Some('"'), Some('"')) | (Some('`'), Some('`')) | (Some('['), Some(']')) if nome.chars().count() >= 2 => c.as_str()[..c.as_str().len() - 1].to_string(),
        _ => nome.to_string(),
    }
}

/// O nome como entra no SQL: citado só quando precisa (maiúscula, espaço,
/// começa com número…), com as aspas do dialeto.
pub fn nome_sql(dialeto: &str, nome: &str) -> String {
    let simples = !nome.is_empty()
        && !nome.starts_with(|c: char| c.is_ascii_digit())
        && nome.chars().all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit())
        && !sql::palavra_chave(nome);
    if simples {
        return nome.to_string();
    }
    match dialeto {
        "mysql" => format!("`{}`", nome.replace('`', "``")),
        "sqlserver" => format!("[{}]", nome.replace(']', "]]")),
        _ => format!("\"{}\"", nome.replace('"', "\"\"")),
    }
}

/// As palavras do dialeto: as do realce mais funções comuns, sem as que
/// o dialeto não tem (TOP só no SQL Server, LIMIT fora dele…).
pub fn palavras(dialeto: &str) -> Vec<&'static str> {
    const FUNCOES: [&str; 8] = ["AVG", "COALESCE", "COUNT", "LOWER", "MAX", "MIN", "SUM", "UPPER"];
    let extras: &[&str] = match dialeto {
        "postgres" => &["INTERVAL", "LATERAL", "NOW"],
        "mysql" => &["INTERVAL", "NOW"],
        "sqlserver" => &["GETDATE", "NOLOCK"],
        _ => &["DATETIME"],
    };
    let so: [(&str, &[&str]); 7] = [
        ("TOP", &["sqlserver"]),
        ("EXEC", &["sqlserver"]),
        ("LIMIT", &["postgres", "mysql", "sqlite"]),
        ("ILIKE", &["postgres"]),
        ("RETURNING", &["postgres", "sqlite"]),
        ("PRAGMA", &["sqlite"]),
        ("SHOW", &["mysql"]),
    ];
    let mut lista: Vec<&str> = sql::PALAVRAS_CHAVE
        .iter()
        .copied()
        .filter(|p| so.iter().find(|(k, _)| k == p).is_none_or(|(_, ds)| ds.contains(&dialeto)))
        .chain(FUNCOES)
        .chain(extras.iter().copied())
        .collect();
    lista.sort_unstable();
    lista.dedup();
    lista
}

/// As sugestões para o que foi analisado, na ordem do lugar: depois de FROM
/// as tabelas; no resto, as colunas das tabelas da instrução, as tabelas e
/// por último as palavras do SQL.
pub fn sugerir(dialeto: &str, a: &Analise, catalogo: &Catalogo) -> Vec<Sugestao> {
    let mut lista = Lista { dialeto, prefixo: a.prefixo.to_lowercase(), itens: Vec::new() };
    let mut tabelas: Vec<&(String, String)> = catalogo.tabelas.iter().collect();
    tabelas.sort_by_key(|(_, t)| t.to_lowercase());
    let colunas_de = |tabela: &str| catalogo.colunas.get(&tabela.to_lowercase()).into_iter().flatten();
    match &a.onde {
        Onde::Depois(q) if catalogo.esquema(q) => {
            for (e, t) in tabelas.iter().filter(|(e, _)| e.eq_ignore_ascii_case(q)) {
                lista.por(t, Especie::Tabela, e);
            }
        }
        Onde::Depois(t) => {
            for (c, tipo) in colunas_de(t) {
                lista.por(c, Especie::Coluna, tipo);
            }
        }
        Onde::Tabela => {
            for (e, t) in &tabelas {
                lista.por(t, Especie::Tabela, e);
            }
            for e in &catalogo.esquemas {
                lista.por(e, Especie::Esquema, "esquema");
            }
        }
        Onde::Geral => {
            if lista.prefixo.is_empty() {
                return Vec::new();
            }
            for t in &a.tabelas {
                for (c, tipo) in colunas_de(t) {
                    lista.por(c, Especie::Coluna, tipo);
                }
            }
            for (e, t) in &tabelas {
                lista.por(t, Especie::Tabela, e);
            }
            // A palavra do SQL na caixa em que você está escrevendo.
            let maiuscula = a.prefixo.chars().any(|c| c.is_uppercase());
            for p in palavras(dialeto) {
                let p = if maiuscula { p.to_string() } else { p.to_lowercase() };
                lista.por(&p, Especie::Palavra, "");
            }
        }
    }
    lista.itens.truncate(MAXIMO);
    lista.itens
}

/// As sugestões juntadas na ordem: só as que começam com o prefixo, sem a
/// palavra já completa e sem repetir.
struct Lista<'a> {
    dialeto: &'a str,
    prefixo: String,
    itens: Vec<Sugestao>,
}

impl Lista<'_> {
    fn por(&mut self, nome: &str, especie: Especie, detalhe: &str) {
        let minusculo = nome.to_lowercase();
        if self.itens.len() >= MAXIMO
            || !minusculo.starts_with(&self.prefixo)
            || minusculo == self.prefixo
            || self.itens.iter().any(|s| s.texto.to_lowercase() == minusculo)
        {
            return;
        }
        let texto = if especie == Especie::Palavra { nome.to_string() } else { nome_sql(self.dialeto, nome) };
        self.itens.push(Sugestao { texto, especie, detalhe: detalhe.to_string() });
    }
}

/// As tabelas cujas colunas a sugestão quer e o catálogo ainda não tem
/// (até 3): a de `nome.` ou as da instrução.
pub fn colunas_que_faltam(a: &Analise, catalogo: &Catalogo) -> Vec<(String, String)> {
    let alvo: Vec<&String> = match &a.onde {
        Onde::Depois(t) if !catalogo.esquema(t) => vec![t],
        Onde::Geral => a.tabelas.iter().collect(),
        _ => Vec::new(),
    };
    alvo.into_iter().filter(|t| !catalogo.colunas.contains_key(&t.to_lowercase())).filter_map(|t| catalogo.tabela(t).cloned()).take(3).collect()
}

/// Troca a palavra em edição (de `inicio` ao `cursor`, em bytes) pela
/// sugestão. Devolve o texto novo e o cursor (em bytes) depois dela.
pub fn aceitar(texto: &str, inicio: usize, cursor: usize, sugestao: &str) -> (String, usize) {
    let mut novo = String::with_capacity(texto.len() + sugestao.len());
    novo.push_str(&texto[..inicio]);
    novo.push_str(sugestao);
    let depois = novo.len();
    novo.push_str(&texto[cursor..]);
    (novo, depois)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn catalogo() -> Catalogo {
        let mut c = Catalogo {
            esquemas: vec!["public".into(), "vendas".into()],
            tabelas: vec![("public".into(), "pedidos".into()), ("public".into(), "clientes".into()), ("vendas".into(), "pagamentos".into())],
            ..Default::default()
        };
        c.colunas.insert("pedidos".into(), vec![("id".into(), "int4".into()), ("cliente_id".into(), "int4".into()), ("total".into(), "numeric".into())]);
        c
    }

    fn textos(s: &[Sugestao]) -> Vec<&str> {
        s.iter().map(|s| s.texto.as_str()).collect()
    }

    fn sugerir_no_fim(dialeto: &str, texto: &str) -> Vec<Sugestao> {
        analisar(dialeto, texto, texto.len()).map(|a| sugerir(dialeto, &a, &catalogo())).unwrap_or_default()
    }

    #[test]
    fn depois_de_from_vem_tabela() {
        let s = sugerir_no_fim("postgres", "SELECT * FROM p");
        assert_eq!(textos(&s), ["pagamentos", "pedidos", "public"]);
        assert_eq!(s[0].especie, Especie::Tabela);
        assert_eq!(s[2].especie, Especie::Esquema);
    }

    #[test]
    fn apelido_e_esquema_depois_do_ponto() {
        let a = analisar("postgres", "SELECT p. FROM pedidos p", 9).unwrap();
        assert_eq!(a.onde, Onde::Depois("pedidos".into()));
        assert_eq!(a.prefixo, "");
        assert_eq!(textos(&sugerir("postgres", &a, &catalogo())), ["id", "cliente_id", "total"]);
        let a = analisar("postgres", "SELECT o.to FROM public.pedidos AS o", 11).unwrap();
        assert_eq!(textos(&sugerir("postgres", &a, &catalogo())), ["total"]);
        assert_eq!(textos(&sugerir_no_fim("postgres", "SELECT * FROM vendas.")), ["pagamentos"]);
    }

    #[test]
    fn geral_junta_colunas_palavras_e_tabelas_na_caixa_escrita() {
        let s = sugerir_no_fim("postgres", "SELECT * FROM pedidos WHERE c");
        assert_eq!(s[0].texto, "cliente_id");
        assert_eq!(s[0].detalhe, "int4");
        assert_eq!(s[1].texto, "clientes");
        assert!(textos(&s).contains(&"case"));
        let s = sugerir_no_fim("postgres", "SEL");
        assert_eq!(textos(&s), ["SELECT"]);
        // Nada depois de um espaço, dentro de texto ou de comentário.
        assert!(analisar("postgres", "SELECT ", 7).is_none());
        assert!(analisar("postgres", "SELECT 'ped", 11).is_none());
        assert!(analisar("postgres", "-- sel", 6).is_none());
        // A palavra já completa não é sugerida de novo.
        assert!(sugerir_no_fim("postgres", "SELECT").is_empty());
    }

    #[test]
    fn palavras_do_dialeto() {
        assert!(palavras("sqlserver").contains(&"TOP") && !palavras("sqlserver").contains(&"LIMIT"));
        assert!(palavras("postgres").contains(&"ILIKE") && !palavras("postgres").contains(&"TOP"));
        assert!(palavras("sqlite").contains(&"PRAGMA") && !palavras("mysql").contains(&"PRAGMA"));
        assert!(palavras("mysql").contains(&"COUNT"));
    }

    #[test]
    fn nomes_citados_pelo_dialeto() {
        assert_eq!(nome_sql("postgres", "pedidos"), "pedidos");
        assert_eq!(nome_sql("postgres", "Pedidos"), "\"Pedidos\"");
        assert_eq!(nome_sql("mysql", "itens pedido"), "`itens pedido`");
        assert_eq!(nome_sql("sqlserver", "Pedidos"), "[Pedidos]");
        assert_eq!(nome_sql("postgres", "order"), "\"order\"");
    }

    #[test]
    fn tabelas_com_apelido_esquema_e_virgula() {
        let t = tabelas_citadas("postgres", "SELECT * FROM loja.pedidos p JOIN \"Clientes\" AS c ON c.id = p.cliente_id, itens WHERE 1 = 1");
        assert_eq!(t, vec![("pedidos".into(), Some("p".into())), ("Clientes".into(), Some("c".into()))]);
        let t = tabelas_citadas("postgres", "SELECT * FROM a, b x WHERE 1");
        assert_eq!(t, vec![("a".into(), None), ("b".into(), Some("x".into()))]);
    }

    #[test]
    fn colunas_pedidas_so_quando_faltam() {
        let c = catalogo();
        let a = analisar("postgres", "SELECT x. FROM clientes x", 9).unwrap();
        assert_eq!(colunas_que_faltam(&a, &c), vec![("public".to_string(), "clientes".to_string())]);
        let a = analisar("postgres", "SELECT p. FROM pedidos p", 9).unwrap();
        assert!(colunas_que_faltam(&a, &c).is_empty());
        // Tabela que a árvore não carregou: nada a pedir.
        let a = analisar("postgres", "SELECT z. FROM sumiu z", 9).unwrap();
        assert!(colunas_que_faltam(&a, &c).is_empty());
    }

    #[test]
    fn aceitar_troca_so_a_palavra() {
        let (t, cursor) = aceitar("SELECT * FROM ped WHERE", 14, 17, "pedidos");
        assert_eq!(t, "SELECT * FROM pedidos WHERE");
        assert_eq!(cursor, 21);
        let t = "SELECT ação.";
        let (t, _) = aceitar(t, t.len(), t.len(), "id");
        assert_eq!(t, "SELECT ação.id");
    }
}
