//! Leitor léxico de SQL da tela: o mesmo do núcleo (`bancos/instrucao.go`),
//! só para escolher a instrução sob o cursor e colorir o editor. Quem decide
//! se uma instrução lê ou altera é sempre o núcleo.

use eframe::egui::{self, Color32, FontId, text::LayoutJob};

use crate::tema::Paleta;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tipo {
    Espaco,
    Palavra,
    Texto,
    Citado,
    Numero,
    Comentario,
    Simbolo,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pedaco {
    pub tipo: Tipo,
    pub ini: usize,
    pub fim: usize,
}

fn alnum(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphanumeric()
}

fn fim_da_linha(sql: &[u8], i: usize) -> usize {
    sql[i..].iter().position(|&c| c == b'\n').map_or(sql.len(), |n| i + n)
}

fn fim_do_comentario(dialeto: &str, sql: &[u8], mut i: usize) -> usize {
    let mut nivel = 0;
    while i < sql.len() {
        if sql[i..].starts_with(b"/*") {
            if nivel == 0 || dialeto == "postgres" || dialeto == "sqlserver" {
                nivel += 1;
            }
            i += 2;
        } else if sql[i..].starts_with(b"*/") {
            nivel -= 1;
            i += 2;
            if nivel <= 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    sql.len()
}

fn fim_do_texto(sql: &[u8], mut i: usize, fecha: u8, barra: bool) -> usize {
    i += 1;
    while i < sql.len() {
        if barra && sql[i] == b'\\' {
            i += 2;
        } else if sql[i] == fecha {
            if i + 1 < sql.len() && sql[i + 1] == fecha {
                i += 2;
                continue;
            }
            return i + 1;
        } else {
            i += 1;
        }
    }
    sql.len()
}

fn fim_do_cifrao(sql: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < sql.len() && alnum(sql[j]) {
        j += 1;
    }
    if j >= sql.len() || sql[j] != b'$' {
        return None;
    }
    let marca = &sql[i..=j];
    let resto = &sql[j + 1..];
    Some(resto.windows(marca.len()).position(|w| w == marca).map_or(sql.len(), |n| j + 1 + n + marca.len()))
}

/// Divide o SQL em pedaços (posições em bytes; nunca no meio de um caractere).
pub fn pedacos(dialeto: &str, texto: &str) -> Vec<Pedaco> {
    let sql = texto.as_bytes();
    let mut lista: Vec<Pedaco> = Vec::new();
    let mut i = 0;
    let mut executavel = false;
    while i < sql.len() {
        let c = sql[i];
        let ini = i;
        let mut tipo = Tipo::Simbolo;
        if c.is_ascii_whitespace() {
            while i < sql.len() && sql[i].is_ascii_whitespace() {
                i += 1;
            }
            tipo = Tipo::Espaco;
        } else if executavel && sql[i..].starts_with(b"*/") {
            i += 2;
            executavel = false;
            tipo = Tipo::Espaco;
        } else if (sql[i..].starts_with(b"--") && (dialeto != "mysql" || i + 2 >= sql.len() || sql[i + 2] <= b' ')) || (c == b'#' && dialeto == "mysql") {
            i = fim_da_linha(sql, i);
            tipo = Tipo::Comentario;
        } else if sql[i..].starts_with(b"/*") {
            if dialeto == "mysql" && i + 2 < sql.len() && sql[i + 2] == b'!' {
                i += 3;
                while i < sql.len() && sql[i].is_ascii_digit() {
                    i += 1;
                }
                executavel = true;
                tipo = Tipo::Espaco;
            } else {
                i = fim_do_comentario(dialeto, sql, i);
                tipo = Tipo::Comentario;
            }
        } else if c == b'\'' {
            let barra = dialeto == "mysql" || (i > 0 && (sql[i - 1] == b'E' || sql[i - 1] == b'e') && (i == 1 || !alnum(sql[i - 2])));
            i = fim_do_texto(sql, i, b'\'', barra);
            tipo = Tipo::Texto;
        } else if c == b'"' {
            i = fim_do_texto(sql, i, b'"', dialeto == "mysql");
            tipo = if dialeto == "mysql" { Tipo::Texto } else { Tipo::Citado };
        } else if c == b'`' && (dialeto == "mysql" || dialeto == "sqlite") {
            i = fim_do_texto(sql, i, b'`', false);
            tipo = Tipo::Citado;
        } else if c == b'[' && (dialeto == "sqlserver" || dialeto == "sqlite") {
            i = fim_do_texto(sql, i, b']', false);
            tipo = Tipo::Citado;
        } else if c == b'$' && dialeto == "postgres" && !(i > 0 && alnum(sql[i - 1])) && !(i + 1 < sql.len() && sql[i + 1].is_ascii_digit()) {
            match fim_do_cifrao(sql, i) {
                Some(fim) => {
                    i = fim;
                    tipo = Tipo::Texto;
                }
                None => i += 1,
            }
        } else if c.is_ascii_digit() {
            while i < sql.len() && (alnum(sql[i]) || sql[i] == b'.') {
                i += 1;
            }
            tipo = Tipo::Numero;
        } else {
            let ch = texto[i..].chars().next().unwrap_or(' ');
            if ch == '_' || ch == '@' || ch == '#' || ch.is_alphabetic() {
                for ch in texto[i..].chars() {
                    if !(ch == '_' || ch == '$' || ch == '@' || ch == '#' || ch.is_alphanumeric()) {
                        break;
                    }
                    i += ch.len_utf8();
                }
                tipo = Tipo::Palavra;
            } else {
                i += ch.len_utf8();
            }
        }
        // Um texto ou comentário pode terminar no meio de um caractere (barra antes do fim).
        while i < texto.len() && !texto.is_char_boundary(i) {
            i += 1;
        }
        let i_fim = i.min(texto.len());
        if tipo == Tipo::Espaco
            && let Some(ultimo) = lista.last_mut()
            && ultimo.tipo == Tipo::Espaco
        {
            ultimo.fim = i_fim;
            continue;
        }
        lista.push(Pedaco { tipo, ini, fim: i_fim });
        i = i_fim;
    }
    lista
}

/// As instruções do texto (sem espaços em volta e sem o ;), em bytes.
pub fn dividir(dialeto: &str, sql: &str) -> Vec<std::ops::Range<usize>> {
    trechos(dialeto, sql, false)
}

/// Divide no `;` e, com `linha_em_branco`, também numa linha em branco fora
/// de texto e comentário (como no IntelliJ: o que vem colado do histórico
/// sem `;` não se junta à instrução de cima).
fn trechos(dialeto: &str, sql: &str, linha_em_branco: bool) -> Vec<std::ops::Range<usize>> {
    let mut trechos = Vec::new();
    let mut atual: Option<std::ops::Range<usize>> = None;
    for p in pedacos(dialeto, sql) {
        match p.tipo {
            Tipo::Simbolo if &sql[p.ini..p.fim] == ";" => trechos.extend(atual.take()),
            Tipo::Espaco if linha_em_branco && sql[p.ini..p.fim].matches('\n').count() >= 2 => trechos.extend(atual.take()),
            Tipo::Espaco | Tipo::Comentario => {}
            _ => match &mut atual {
                Some(r) => r.end = p.fim,
                None => atual = Some(p.ini..p.fim),
            },
        }
    }
    trechos.extend(atual);
    trechos
}

/// A instrução do cursor (em bytes): vai até o `;` ou até uma linha em
/// branco. Fora de todas, a última antes dele; sem ela, a primeira depois.
pub fn sob_cursor(dialeto: &str, sql: &str, pos: usize) -> Option<std::ops::Range<usize>> {
    let trechos = trechos(dialeto, sql, true);
    let mut antes = None;
    for t in &trechos {
        if pos >= t.start && pos <= t.end {
            return Some(t.clone());
        }
        if t.end <= pos {
            antes = Some(t.clone());
        }
    }
    antes.or_else(|| trechos.first().cloned())
}

/// Posição em bytes de um índice de caractere (o cursor do egui conta caracteres).
pub fn byte_do_caractere(texto: &str, caractere: usize) -> usize {
    texto.char_indices().nth(caractere).map_or(texto.len(), |(i, _)| i)
}

pub fn caractere_do_byte(texto: &str, byte: usize) -> usize {
    texto[..byte.min(texto.len())].chars().count()
}

const PALAVRAS_CHAVE: &[&str] = &[
    "ADD",
    "ALL",
    "ALTER",
    "ANALYZE",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "BEGIN",
    "BETWEEN",
    "BY",
    "CASE",
    "CAST",
    "CHECK",
    "COLUMN",
    "COMMIT",
    "CONSTRAINT",
    "CREATE",
    "CROSS",
    "CURRENT_DATE",
    "CURRENT_TIMESTAMP",
    "DATABASE",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DESCRIBE",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "EXCEPT",
    "EXEC",
    "EXECUTE",
    "EXISTS",
    "EXPLAIN",
    "FALSE",
    "FETCH",
    "FOR",
    "FOREIGN",
    "FROM",
    "FULL",
    "GRANT",
    "GROUP",
    "HAVING",
    "ILIKE",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "LEFT",
    "LIKE",
    "LIMIT",
    "MERGE",
    "NOT",
    "NULL",
    "OFFSET",
    "ON",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PARTITION",
    "PRAGMA",
    "PRIMARY",
    "RECURSIVE",
    "REFERENCES",
    "RETURNING",
    "REVOKE",
    "RIGHT",
    "ROLLBACK",
    "SCHEMA",
    "SELECT",
    "SET",
    "SHOW",
    "TABLE",
    "THEN",
    "TOP",
    "TRUE",
    "TRUNCATE",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USING",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
];

fn palavra_chave(palavra: &str) -> bool {
    palavra.len() <= 17 && PALAVRAS_CHAVE.binary_search(&palavra.to_ascii_uppercase().as_str()).is_ok()
}

/// O texto colorido: palavras-chave em destaque, textos em verde, números
/// no azul do terminal, comentários apagados, o resto na cor do texto.
/// `marcado` ganha o fundo do trecho que acabou de ser executado.
pub fn realce(dialeto: &str, sql: &str, fonte: FontId, p: &Paleta, marcado: Option<(std::ops::Range<usize>, Color32)>) -> LayoutJob {
    let mut job = LayoutJob::default();
    for pedaco in pedacos(dialeto, sql) {
        let trecho = &sql[pedaco.ini..pedaco.fim];
        let cor = match pedaco.tipo {
            Tipo::Palavra if palavra_chave(trecho) => p.destaque,
            Tipo::Texto => p.ok,
            Tipo::Numero => p.ansi[4],
            Tipo::Comentario => p.suave,
            _ => p.texto,
        };
        let mut formato = egui::TextFormat::simple(fonte.clone(), cor);
        match &marcado {
            Some((r, fundo)) if pedaco.ini < r.end && pedaco.fim > r.start => {
                // O pedaço pode cruzar a borda do trecho: corta em três.
                let (a, b) = (r.start.max(pedaco.ini), r.end.min(pedaco.fim));
                if a > pedaco.ini {
                    job.append(&sql[pedaco.ini..a], 0.0, formato.clone());
                }
                let mut com_fundo = formato.clone();
                com_fundo.background = *fundo;
                job.append(&sql[a..b], 0.0, com_fundo);
                if b < pedaco.fim {
                    job.append(&sql[b..pedaco.fim], 0.0, formato);
                }
                continue;
            }
            _ => {}
        }
        formato.color = cor;
        job.append(trecho, 0.0, formato);
    }
    job
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn palavras_em_ordem_para_a_busca() {
        let mut ordenadas = PALAVRAS_CHAVE.to_vec();
        ordenadas.sort();
        assert_eq!(ordenadas, PALAVRAS_CHAVE);
    }

    #[test]
    fn divide_sem_cortar_texto_nem_comentario() {
        let sql = "SELECT 1;\n\n-- dois;\nSELECT 'a;b' ;\nUPDATE x SET y = 1";
        let trechos: Vec<&str> = dividir("postgres", sql).into_iter().map(|r| &sql[r]).collect();
        assert_eq!(trechos, ["SELECT 1", "SELECT 'a;b'", "UPDATE x SET y = 1"]);
        assert_eq!(&sql[sob_cursor("postgres", sql, 10).unwrap()], "SELECT 1");
        assert_eq!(&sql[sob_cursor("postgres", sql, sql.len()).unwrap()], "UPDATE x SET y = 1");
        assert!(sob_cursor("postgres", "  -- nada", 3).is_none());
        // Sem ;, uma linha em branco separa as instruções sob o cursor.
        let colado = "select pg_sleep(20)\n\nselect id from clientes";
        assert_eq!(&colado[sob_cursor("postgres", colado, colado.len()).unwrap()], "select id from clientes");
        assert_eq!(&colado[sob_cursor("postgres", colado, 3).unwrap()], "select pg_sleep(20)");
        let texto = "select 'a\n\nb'";
        assert_eq!(&texto[sob_cursor("postgres", texto, texto.len()).unwrap()], texto);
        let mysql = "SELECT `a;b`, \"c;d\" # e;f\n FROM x; SELECT 2";
        assert_eq!(dividir("mysql", mysql).len(), 2);
        let pg = "SELECT $x$ ; $x$, E'\\';' ; SELECT 3";
        assert_eq!(dividir("postgres", pg).len(), 2);
    }

    #[test]
    fn acentos_e_texto_sem_fim_nao_quebram() {
        for sql in ["SELECT 'ação", "SELECT \"é", "/* ã", "SELECT ç, ñ FROM ü", "$", "'\\"] {
            for d in ["postgres", "mysql", "sqlserver", "sqlite"] {
                let p = pedacos(d, sql);
                assert_eq!(p.last().map(|p| p.fim), Some(sql.len()), "{d} {sql}");
                for x in &p {
                    assert!(sql.is_char_boundary(x.ini) && sql.is_char_boundary(x.fim));
                }
            }
        }
        assert_eq!(byte_do_caractere("ação x", 4), 6);
        assert_eq!(caractere_do_byte("ação x", 6), 4);
    }

    #[test]
    fn realce_marca_o_trecho() {
        let p = crate::tema::cores();
        let job = realce("postgres", "SELECT 1 FROM x", FontId::monospace(13.0), p, Some((0..8, Color32::RED)));
        assert_eq!(job.text, "SELECT 1 FROM x");
        assert!(job.sections.iter().any(|s| s.format.background == Color32::RED));
        assert_eq!(job.sections[0].format.color, p.destaque);
    }
}
