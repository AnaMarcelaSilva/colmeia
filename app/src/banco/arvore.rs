//! A árvore das conexões: pasta › conexão › banco › esquema › Tabelas/Views ›
//! tabela › colunas. Cada nível é pedido ao núcleo quando abre (e fica em
//! cache até "Atualizar"); o filtro corta pelo nome o que já foi carregado.
//! A lista é virtual (`show_rows`): 1007 tabelas não pesam.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Align2, CornerRadius, FontId, Id, Rect, Sense, Stroke, pos2, vec2};

use crate::api;
use crate::registro;
use crate::tema::{self, Icone, cores, forte};

pub const ALTURA_LINHA: f32 = 26.0;

/// O erro de um nível que espera a senha (clicar nele abre o diálogo).
pub const PEDE_SENHA: &str = "Digite a senha da conexão.";

/// O servidor recusou a senha (trocada lá, ou conexão salva sem senha):
/// clicar na linha também abre o diálogo de senha.
pub const SENHA_RECUSADA: &str = "Usuário ou senha recusados.";

/// O erro se resolve digitando a senha (falta ou foi recusada).
fn erro_de_senha(texto: &str) -> bool {
    texto == PEDE_SENHA || texto == SENHA_RECUSADA
}

/// Um nível carregando, pronto ou com erro.
pub enum Carga<T> {
    Carregando,
    Pronta(T),
    Erro(String),
}

/// Onde um nó fica: a conexão, o banco, o esquema e o objeto (vazios quando
/// o tipo de banco não tem o nível).
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum No {
    Pasta(String),
    Conexao(i64),
    Banco(i64, String),
    Esquema(i64, String, String),
    /// Tabelas (false) ou Views (true) de um esquema.
    Grupo(i64, String, String, bool),
    Tabela(i64, String, String, String),
}

impl No {
    pub fn conexao(&self) -> i64 {
        match self {
            No::Pasta(_) => 0,
            No::Conexao(c) | No::Banco(c, _) | No::Esquema(c, ..) | No::Grupo(c, ..) | No::Tabela(c, ..) => *c,
        }
    }
}

/// O que a árvore pede ao núcleo (a tela faz numa thread).
#[derive(Clone, Debug, PartialEq)]
pub enum Pedido {
    Bancos(i64),
    Esquemas(i64, String),
    Objetos(i64, String, String),
    Colunas(i64, String, String, String),
}

/// O que a árvore pede à tela.
#[derive(Clone, Debug, PartialEq)]
pub enum Acao {
    Pedir(Pedido),
    Escolher(i64),
    Previa(i64, String, String, String),
    GerarSelect(i64, String, String, String),
    CopiarNome(String),
    Editar(i64),
    Duplicar(i64),
    Testar(i64),
    Atualizar(i64),
    Desconectar(i64),
    Remover(i64),
    TrocarSenha(i64),
}

#[derive(Default)]
pub struct Arvore {
    pub abertos: HashSet<No>,
    pub pastas_fechadas: HashSet<String>,
    pub bancos: HashMap<i64, Carga<api::Nomes>>,
    /// Conexões com banco padrão em que você pediu "Mostrar todos".
    pub todos: HashSet<i64>,
    pub esquemas: HashMap<(i64, String), Carga<Vec<String>>>,
    pub objetos: HashMap<(i64, String, String), Carga<api::Objetos>>,
    pub colunas: HashMap<(i64, String, String, String), Carga<Vec<api::ColunaTabela>>>,
    /// Erro ao abrir a conexão (vai na dica e no ícone).
    pub erros: HashMap<i64, String>,
    pub filtro: String,
    pub escolhido: Option<No>,
    pub focar_filtro: bool,
}

/// Uma linha já achatada, emprestando os nomes da árvore.
#[derive(Clone, Debug)]
enum Linha<'a> {
    Pasta { nome: &'a str, n: usize, aberta: bool },
    Conexao { c: &'a api::ConexaoBanco, nivel: usize, aberta: bool, carregando: bool, erro: Option<&'a str> },
    Banco { con: i64, nome: &'a str, nivel: usize, aberta: bool, carregando: bool },
    MostrarTodos { con: i64, nivel: usize },
    Esquema { con: i64, banco: &'a str, nome: &'a str, nivel: usize, aberta: bool, carregando: bool },
    Grupo { con: i64, banco: &'a str, esquema: &'a str, views: bool, nivel: usize, aberta: bool, n: usize, de: usize },
    Tabela { con: i64, banco: &'a str, esquema: &'a str, nome: &'a str, view: bool, nivel: usize, aberta: bool, carregando: bool },
    Coluna { coluna: &'a api::ColunaTabela, nivel: usize },
    Carregando { nivel: usize },
    Erro { con: i64, texto: &'a str, nivel: usize },
    Cortado { mostrando: usize, total: usize, nivel: usize },
    Vazio { texto: &'static str, nivel: usize },
}

impl Arvore {
    /// Esquece o cache da conexão (Atualizar, editar, trocar a senha).
    pub fn esquecer(&mut self, con: i64) {
        self.bancos.remove(&con);
        self.esquemas.retain(|k, _| k.0 != con);
        self.objetos.retain(|k, _| k.0 != con);
        self.colunas.retain(|k, _| k.0 != con);
        self.erros.remove(&con);
    }

    /// Esquece o cache da conexão e pede de novo os níveis que estão abertos
    /// (Atualizar, senha aceita, conexão editada).
    pub fn atualizar(&mut self, c: i64, tipo: &str, padrao: &str, acoes: &mut Vec<Acao>) {
        self.esquecer(c);
        if self.abertos.contains(&No::Conexao(c))
            && let Some(p) = self.pedido_ao_abrir(&No::Conexao(c), tipo, padrao)
        {
            acoes.push(Acao::Pedir(p));
        }
        let abertos: Vec<No> = self.abertos.iter().filter(|n| n.conexao() == c && !matches!(n, No::Conexao(_))).cloned().collect();
        for n in abertos {
            if let Some(p) = self.pedido_ao_abrir(&n, tipo, padrao) {
                acoes.push(Acao::Pedir(p));
            }
        }
    }

    /// Abre (ou fecha) um nó; abrir pede o que falta ao núcleo.
    fn alternar(&mut self, no: No, tipo: &str, padrao: &str, acoes: &mut Vec<Acao>) {
        if !self.abertos.remove(&no) {
            // Abrir de novo uma conexão que deu erro tenta outra vez (sem
            // senha, o núcleo responde 428 e o diálogo de senha volta).
            if let No::Conexao(c) = no
                && self.erros.contains_key(&c)
            {
                self.esquecer(c);
            }
            self.abertos.insert(no.clone());
            if let Some(p) = self.pedido_ao_abrir(&no, tipo, padrao) {
                acoes.push(Acao::Pedir(p));
            }
        }
    }

    fn pedido_ao_abrir(&mut self, no: &No, tipo: &str, padrao: &str) -> Option<Pedido> {
        match no {
            No::Conexao(c) => match tipo {
                // No SQLite a tabela fica logo abaixo da conexão.
                "sqlite" if !self.objetos.contains_key(&(*c, String::new(), String::new())) => {
                    self.objetos.insert((*c, String::new(), String::new()), Carga::Carregando);
                    Some(Pedido::Objetos(*c, String::new(), String::new()))
                }
                "sqlite" => None,
                // Com banco padrão, a árvore começa só nele (o resto em "Mostrar todos").
                _ if !padrao.is_empty() && !self.todos.contains(c) => {
                    let banco = No::Banco(*c, padrao.to_string());
                    self.abertos.insert(banco.clone());
                    self.pedido_ao_abrir(&banco, tipo, padrao)
                }
                _ if !self.bancos.contains_key(c) => {
                    self.bancos.insert(*c, Carga::Carregando);
                    Some(Pedido::Bancos(*c))
                }
                _ => None,
            },
            No::Banco(c, b) => {
                if tipo == "mysql" {
                    let chave = (*c, b.clone(), String::new());
                    if self.objetos.contains_key(&chave) {
                        return None;
                    }
                    self.objetos.insert(chave, Carga::Carregando);
                    return Some(Pedido::Objetos(*c, b.clone(), String::new()));
                }
                let chave = (*c, b.clone());
                if self.esquemas.contains_key(&chave) {
                    return None;
                }
                self.esquemas.insert(chave, Carga::Carregando);
                Some(Pedido::Esquemas(*c, b.clone()))
            }
            No::Esquema(c, b, e) => {
                let chave = (*c, b.clone(), e.clone());
                if self.objetos.contains_key(&chave) {
                    return None;
                }
                self.objetos.insert(chave, Carga::Carregando);
                Some(Pedido::Objetos(*c, b.clone(), e.clone()))
            }
            No::Tabela(c, b, e, t) => {
                let chave = (*c, b.clone(), e.clone(), t.clone());
                if self.colunas.contains_key(&chave) {
                    return None;
                }
                self.colunas.insert(chave, Carga::Carregando);
                Some(Pedido::Colunas(*c, b.clone(), e.clone(), t.clone()))
            }
            _ => None,
        }
    }

    /// Guarda o que o núcleo respondeu. Um nível com um filho só abre sozinho.
    pub fn receber(&mut self, pedido: Pedido, resposta: Result<Resposta, String>, tipo: &str, acoes: &mut Vec<Acao>) {
        match (pedido, resposta) {
            (Pedido::Bancos(c), Ok(Resposta::Bancos(n))) => {
                self.erros.remove(&c);
                if n.nomes.len() == 1 {
                    let no = No::Banco(c, n.nomes[0].clone());
                    if self.abertos.insert(no.clone())
                        && let Some(p) = self.pedido_ao_abrir(&no, tipo, "")
                    {
                        acoes.push(Acao::Pedir(p));
                    }
                }
                self.bancos.insert(c, Carga::Pronta(n));
            }
            (Pedido::Esquemas(c, b), Ok(Resposta::Nomes(nomes))) => {
                self.erros.remove(&c);
                if nomes.len() == 1 {
                    let no = No::Esquema(c, b.clone(), nomes[0].clone());
                    if self.abertos.insert(no.clone())
                        && let Some(p) = self.pedido_ao_abrir(&no, tipo, "")
                    {
                        acoes.push(Acao::Pedir(p));
                    }
                }
                self.esquemas.insert((c, b), Carga::Pronta(nomes));
            }
            (Pedido::Objetos(c, b, e), Ok(Resposta::Objetos(o))) => {
                self.erros.remove(&c);
                // Só tabelas: o grupo Tabelas já vem aberto.
                if o.views.is_empty() || o.tabelas.is_empty() {
                    self.abertos.insert(No::Grupo(c, b.clone(), e.clone(), o.tabelas.is_empty()));
                }
                self.objetos.insert((c, b, e), Carga::Pronta(o));
            }
            (Pedido::Colunas(c, b, e, t), Ok(Resposta::Colunas(cs))) => {
                self.colunas.insert((c, b, e, t), Carga::Pronta(cs));
            }
            (p, Err(e)) => {
                let c = match &p {
                    Pedido::Bancos(c) | Pedido::Esquemas(c, _) | Pedido::Objetos(c, ..) | Pedido::Colunas(c, ..) => *c,
                };
                match p {
                    Pedido::Bancos(c) => {
                        self.bancos.insert(c, Carga::Erro(e.clone()));
                        self.erros.insert(c, e);
                    }
                    Pedido::Esquemas(c, b) => {
                        self.esquemas.insert((c, b), Carga::Erro(e));
                    }
                    Pedido::Objetos(c, b, s) => {
                        if b.is_empty() && s.is_empty() {
                            self.erros.insert(c, e.clone());
                        }
                        self.objetos.insert((c, b, s), Carga::Erro(e));
                    }
                    Pedido::Colunas(c, b, s, t) => {
                        self.colunas.insert((c, b, s, t), Carga::Erro(e));
                    }
                }
                let _ = c;
            }
            _ => {}
        }
    }

    fn passa(&self, nome: &str) -> bool {
        self.filtro.is_empty() || nome.to_lowercase().contains(&self.filtro.to_lowercase())
    }

    /// Achata a árvore na ordem em que aparece.
    fn linhas<'a>(&'a self, conexoes: &'a [api::ConexaoBanco]) -> Vec<Linha<'a>> {
        let mut saida = Vec::new();
        let filtrando = !self.filtro.is_empty();
        let mut pasta_atual: Option<&str> = None;
        for c in conexoes {
            let mut nivel = 0;
            if !c.pasta.is_empty() {
                if pasta_atual != Some(c.pasta.as_str()) {
                    pasta_atual = Some(c.pasta.as_str());
                    let n = conexoes.iter().filter(|x| x.pasta == c.pasta).count();
                    saida.push(Linha::Pasta { nome: &c.pasta, n, aberta: !self.pastas_fechadas.contains(&c.pasta) });
                }
                if self.pastas_fechadas.contains(&c.pasta) {
                    continue;
                }
                nivel = 1;
            } else {
                pasta_atual = None;
            }
            let aberta = self.abertos.contains(&No::Conexao(c.id));
            let carregando = matches!(self.bancos.get(&c.id), Some(Carga::Carregando))
                || matches!(self.objetos.get(&(c.id, String::new(), String::new())), Some(Carga::Carregando)) && c.tipo == "sqlite";
            saida.push(Linha::Conexao { c, nivel, aberta, carregando, erro: self.erros.get(&c.id).map(String::as_str) });
            if !aberta {
                continue;
            }
            let nivel = nivel + 1;
            if c.tipo == "sqlite" {
                self.objetos_em(&mut saida, c.id, "", "", nivel, filtrando);
                continue;
            }
            // Os bancos: só o padrão (com "Mostrar todos") ou a lista do servidor.
            let so_padrao = !c.banco.is_empty() && !self.todos.contains(&c.id);
            let nomes: Vec<&str> = if so_padrao {
                vec![c.banco.as_str()]
            } else {
                match self.bancos.get(&c.id) {
                    Some(Carga::Pronta(n)) => n.nomes.iter().map(String::as_str).collect(),
                    Some(Carga::Erro(e)) => {
                        saida.push(Linha::Erro { con: c.id, texto: e, nivel });
                        continue;
                    }
                    _ => {
                        saida.push(Linha::Carregando { nivel });
                        continue;
                    }
                }
            };
            for nome in nomes {
                let no = No::Banco(c.id, nome.to_string());
                let aberta = self.abertos.contains(&no);
                let carregando = if c.tipo == "mysql" {
                    matches!(self.objetos.get(&(c.id, nome.to_string(), String::new())), Some(Carga::Carregando))
                } else {
                    matches!(self.esquemas.get(&(c.id, nome.to_string())), Some(Carga::Carregando))
                };
                saida.push(Linha::Banco { con: c.id, nome, nivel, aberta, carregando });
                if !aberta {
                    continue;
                }
                if c.tipo == "mysql" {
                    self.objetos_em(&mut saida, c.id, nome, "", nivel + 1, filtrando);
                    continue;
                }
                match self.esquemas.get(&(c.id, nome.to_string())) {
                    Some(Carga::Pronta(esquemas)) => {
                        for e in esquemas {
                            let no = No::Esquema(c.id, nome.to_string(), e.clone());
                            let aberta = self.abertos.contains(&no);
                            let carregando = matches!(self.objetos.get(&(c.id, nome.to_string(), e.clone())), Some(Carga::Carregando));
                            saida.push(Linha::Esquema { con: c.id, banco: nome, nome: e, nivel: nivel + 1, aberta, carregando });
                            if aberta {
                                self.objetos_em(&mut saida, c.id, nome, e, nivel + 2, filtrando);
                            }
                        }
                    }
                    Some(Carga::Erro(e)) => saida.push(Linha::Erro { con: c.id, texto: e, nivel: nivel + 1 }),
                    _ => saida.push(Linha::Carregando { nivel: nivel + 1 }),
                }
            }
            if so_padrao {
                saida.push(Linha::MostrarTodos { con: c.id, nivel });
            }
        }
        saida
    }

    fn objetos_em<'a>(&'a self, saida: &mut Vec<Linha<'a>>, con: i64, banco: &'a str, esquema: &'a str, nivel: usize, filtrando: bool) {
        let chave = (con, banco.to_string(), esquema.to_string());
        let o = match self.objetos.get(&chave) {
            Some(Carga::Pronta(o)) => o,
            Some(Carga::Erro(e)) => {
                saida.push(Linha::Erro { con, texto: e, nivel });
                return;
            }
            _ => {
                saida.push(Linha::Carregando { nivel });
                return;
            }
        };
        if o.tabelas.is_empty() && o.views.is_empty() {
            saida.push(Linha::Vazio { texto: "Nenhuma tabela", nivel });
            return;
        }
        for (views, nomes) in [(false, &o.tabelas), (true, &o.views)] {
            if nomes.is_empty() {
                continue;
            }
            let passam: Vec<&String> = nomes.iter().filter(|n| self.passa(n)).collect();
            if filtrando && passam.is_empty() {
                continue;
            }
            let no = No::Grupo(con, banco.to_string(), esquema.to_string(), views);
            // Filtrando, os grupos com resultado abrem sozinhos.
            let aberta = filtrando || self.abertos.contains(&no);
            saida.push(Linha::Grupo { con, banco, esquema, views, nivel, aberta, n: passam.len(), de: nomes.len() });
            if !aberta {
                continue;
            }
            for nome in passam {
                let no = No::Tabela(con, banco.to_string(), esquema.to_string(), nome.clone());
                let aberta = self.abertos.contains(&no);
                let chave = (con, banco.to_string(), esquema.to_string(), nome.clone());
                let carregando = matches!(self.colunas.get(&chave), Some(Carga::Carregando));
                saida.push(Linha::Tabela { con, banco, esquema, nome, view: views, nivel: nivel + 1, aberta, carregando });
                if aberta {
                    match self.colunas.get(&chave) {
                        Some(Carga::Pronta(cs)) => saida.extend(cs.iter().map(|coluna| Linha::Coluna { coluna, nivel: nivel + 2 })),
                        Some(Carga::Erro(e)) => saida.push(Linha::Erro { con, texto: e, nivel: nivel + 2 }),
                        _ => saida.push(Linha::Carregando { nivel: nivel + 2 }),
                    }
                }
            }
            if o.cortado && !views {
                saida.push(Linha::Cortado { mostrando: o.tabelas.len() + o.views.len(), total: o.total, nivel: nivel + 1 });
            }
        }
    }

    /// Desenha o filtro e a lista; devolve o que a tela precisa fazer.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, conexoes: &[api::ConexaoBanco]) -> Vec<Acao> {
        let p = cores();
        let mut acoes = Vec::new();
        // Filtro no topo (Ctrl+F foca; o foco é pedido uma vez só).
        let id_filtro = Id::new("filtro-arvore-banco");
        ui.horizontal(|ui| {
            // A mesma largura útil das linhas (que deixam a margem da barra de
            // rolagem e 4 px de cada lado para o fundo da escolhida).
            ui.spacing_mut().item_spacing.x = 0.0;
            let largura = ui.available_width() - registro::MARGEM_ROLAGEM - 8.0;
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(tema::fundo_campo(p, false))
                .stroke(Stroke::new(
                    if ui.memory(|m| m.has_focus(id_filtro)) { 1.5 } else { 1.0 },
                    if ui.memory(|m| m.has_focus(id_filtro)) { p.destaque } else { p.borda },
                ))
                .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let espaco = if self.filtro.is_empty() { 0.0 } else { 28.0 };
                        let campo = egui::TextEdit::singleline(&mut self.filtro)
                            .id(id_filtro)
                            .frame(egui::Frame::NONE)
                            .desired_width((largura - 22.0 - espaco).max(40.0))
                            .font(FontId::proportional(13.5))
                            .hint_text(egui::RichText::new("Filtrar (Ctrl+F)").color(p.suave));
                        let r = ui.add(campo);
                        if std::mem::take(&mut self.focar_filtro) {
                            r.request_focus();
                        }
                        if !self.filtro.is_empty() && tema::botao_icone(ui, Icone::Fechar, 20.0).on_hover_text("Limpar filtro").clicked() {
                            self.filtro.clear();
                        }
                    });
                });
        });
        ui.add_space(8.0);
        let linhas = self.linhas(conexoes);
        if linhas.is_empty() {
            return acoes;
        }
        let tem_tabela = linhas.iter().any(|l| matches!(l, Linha::Tabela { .. }));
        let filtrando = !self.filtro.is_empty();
        let tem_objetos_carregados = !self.objetos.is_empty();
        let mut clique: Option<(usize, Clique)> = None;
        let mut limpar = false;
        let saida = egui::ScrollArea::vertical().id_salt("arvore-banco").auto_shrink(false).show_rows(ui, ALTURA_LINHA, linhas.len(), |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in faixa {
                if let Some(c) = linha(ui, &linhas[i], self.escolhido.as_ref(), &self.filtro, i) {
                    clique = Some((i, c));
                }
            }
        });
        registro::sombra_rolagem(ui.painter(), saida.inner_rect, saida.state.offset.y);
        if filtrando && !tem_tabela && tem_objetos_carregados {
            // Nada casou nos itens já carregados.
            let r = saida.inner_rect;
            let texto = format!("Nada com “{}” nos itens carregados", self.filtro);
            ui.painter().text(r.center() - vec2(0.0, 20.0), Align2::CENTER_CENTER, texto, FontId::proportional(13.0), p.suave);
            let botao = Rect::from_center_size(r.center() + vec2(0.0, 16.0), vec2(120.0, 32.0));
            limpar = ui.put(botao, |ui: &mut egui::Ui| tema::botao_secundario(ui, "Limpar filtro")).clicked();
        }
        let Some((i, clique)) = clique else {
            drop(linhas);
            if limpar {
                self.filtro.clear();
            }
            return acoes;
        };
        let tipo_de = |con: i64| conexoes.iter().find(|c| c.id == con).map(|c| (c.tipo.clone(), c.banco.clone())).unwrap_or_default();
        // As linhas emprestam a árvore: a ação vira dados próprios antes de mexer nela.
        let alvo: Option<No> = match &linhas[i] {
            Linha::Pasta { nome, .. } => Some(No::Pasta(nome.to_string())),
            Linha::Conexao { c, .. } => Some(No::Conexao(c.id)),
            Linha::Banco { con, nome, .. } => Some(No::Banco(*con, nome.to_string())),
            Linha::Esquema { con, banco, nome, .. } => Some(No::Esquema(*con, banco.to_string(), nome.to_string())),
            Linha::Grupo { con, banco, esquema, views, .. } => Some(No::Grupo(*con, banco.to_string(), esquema.to_string(), *views)),
            Linha::Tabela { con, banco, esquema, nome, .. } => Some(No::Tabela(*con, banco.to_string(), esquema.to_string(), nome.to_string())),
            Linha::Erro { con, texto, .. } => {
                // A linha do erro: sem senha ou com a senha recusada, abre o
                // diálogo de senha; outro erro, tenta outra vez.
                let (con, senha) = (*con, erro_de_senha(texto));
                drop(linhas);
                if senha {
                    acoes.push(Acao::TrocarSenha(con));
                } else {
                    let (tipo, padrao) = tipo_de(con);
                    self.atualizar(con, &tipo, &padrao, &mut acoes);
                }
                return acoes;
            }
            Linha::MostrarTodos { con, .. } => {
                let con = *con;
                drop(linhas);
                self.todos.insert(con);
                if let std::collections::hash_map::Entry::Vacant(v) = self.bancos.entry(con) {
                    v.insert(Carga::Carregando);
                    acoes.push(Acao::Pedir(Pedido::Bancos(con)));
                }
                return acoes;
            }
            _ => None,
        };
        drop(linhas);
        let Some(no) = alvo else { return acoes };
        let (tipo, padrao) = tipo_de(no.conexao());
        match clique {
            Clique::Alternar => match &no {
                No::Pasta(nome) => {
                    if !self.pastas_fechadas.remove(nome) {
                        self.pastas_fechadas.insert(nome.clone());
                    }
                }
                _ => {
                    if let No::Conexao(c) = no {
                        acoes.push(Acao::Escolher(c));
                    }
                    self.escolhido = Some(no.clone());
                    self.alternar(no, &tipo, &padrao, &mut acoes);
                }
            },
            Clique::Escolher => {
                acoes.push(Acao::Escolher(no.conexao()));
                self.escolhido = Some(no);
            }
            Clique::Duplo => {
                if let No::Tabela(c, b, e, t) = &no {
                    acoes.push(Acao::Previa(*c, b.clone(), e.clone(), t.clone()));
                }
                acoes.push(Acao::Escolher(no.conexao()));
                self.escolhido = Some(no);
            }
            Clique::Menu(m) => {
                let c = no.conexao();
                match (m, &no) {
                    (Menu::Primeiras, No::Tabela(c, b, e, t)) => acoes.push(Acao::Previa(*c, b.clone(), e.clone(), t.clone())),
                    (Menu::Gerar, No::Tabela(c, b, e, t)) => acoes.push(Acao::GerarSelect(*c, b.clone(), e.clone(), t.clone())),
                    (Menu::CopiarNome, No::Tabela(.., t)) => acoes.push(Acao::CopiarNome(t.clone())),
                    (Menu::Console, _) => acoes.push(Acao::Escolher(c)),
                    (Menu::Editar, _) => acoes.push(Acao::Editar(c)),
                    (Menu::Duplicar, _) => acoes.push(Acao::Duplicar(c)),
                    (Menu::Testar, _) => acoes.push(Acao::Testar(c)),
                    (Menu::Atualizar, _) => {
                        self.atualizar(c, &tipo, &padrao, &mut acoes);
                        acoes.push(Acao::Atualizar(c));
                    }
                    (Menu::Desconectar, _) => {
                        self.esquecer(c);
                        self.abertos.retain(|n| n.conexao() != c);
                        acoes.push(Acao::Desconectar(c));
                    }
                    (Menu::Remover, _) => acoes.push(Acao::Remover(c)),
                    (Menu::TrocarSenha, _) => acoes.push(Acao::TrocarSenha(c)),
                    _ => {}
                }
            }
        }
        acoes
    }
}

/// O que o núcleo respondeu a um pedido da árvore.
pub enum Resposta {
    Bancos(api::Nomes),
    Nomes(Vec<String>),
    Objetos(api::Objetos),
    Colunas(Vec<api::ColunaTabela>),
}

enum Clique {
    Alternar,
    Escolher,
    Duplo,
    Menu(Menu),
}

#[derive(Clone, Copy)]
enum Menu {
    Console,
    Editar,
    Duplicar,
    Testar,
    Atualizar,
    Desconectar,
    Remover,
    TrocarSenha,
    Primeiras,
    Gerar,
    CopiarNome,
}

/// Os selos da conexão: o ícone, a cor e a dica.
type Selos = Vec<(Icone, egui::Color32, &'static str)>;

/// Seta de abrir, como na gaveta de arquivos.
fn seta(pintor: &egui::Painter, c: egui::Pos2, aberta: bool, cor: egui::Color32) {
    let pontos = if aberta {
        vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
    } else {
        vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
    };
    pintor.add(egui::Shape::convex_polygon(pontos, cor, Stroke::NONE));
}

/// O nome com o trecho que casou no filtro realçado (fundo, nunca a cor da letra).
fn nome_com_filtro(pintor: &egui::Painter, nome: &str, filtro: &str, fonte: FontId, largura: f32) -> std::sync::Arc<egui::Galley> {
    let p = cores();
    let formato = egui::TextFormat::simple(fonte.clone(), p.texto);
    let achou = if filtro.is_empty() { None } else { nome.to_lowercase().find(&filtro.to_lowercase()) };
    match achou {
        Some(i) if nome.is_char_boundary(i) && nome.is_char_boundary((i + filtro.len()).min(nome.len())) => {
            let fim = (i + filtro.len()).min(nome.len());
            let mut job = egui::text::LayoutJob::default();
            job.append(&nome[..i], 0.0, formato.clone());
            let mut marcado = formato.clone();
            marcado.background = tema::misturar(p.superficie, p.destaque, if tema::claro() { 0.28 } else { 0.22 });
            job.append(&nome[i..fim], 0.0, marcado);
            job.append(&nome[fim..], 0.0, formato);
            job.wrap = egui::text::TextWrapping { max_width: largura.max(20.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
            pintor.layout_job(job)
        }
        _ => tema::cortar(pintor, nome, formato, largura, 1, true),
    }
}

fn linha(ui: &mut egui::Ui, linha: &Linha, escolhido: Option<&No>, filtro: &str, i: usize) -> Option<Clique> {
    let p = cores();
    let largura = ui.available_width() - registro::MARGEM_ROLAGEM;
    let (rect, _) = ui.allocate_exact_size(vec2(largura, ALTURA_LINHA), Sense::hover());
    let pintor = ui.painter().clone();
    let recuo = |nivel: usize| rect.left() + 10.0 + nivel as f32 * 16.0;
    let texto_apagado = |x: f32, texto: &str| {
        let formato = egui::TextFormat { font_id: FontId::proportional(12.5), color: p.suave, italics: true, ..Default::default() };
        let g = pintor.layout_job(egui::text::LayoutJob::single_section(texto.to_string(), formato));
        pintor.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.suave);
    };
    match linha {
        Linha::Carregando { nivel } => {
            tema::esqueleto(&pintor, Rect::from_min_size(pos2(recuo(*nivel) + 16.0, rect.center().y - 5.0), vec2(120.0, 10.0)), tema::RAIO_ETIQUETA);
            return None;
        }
        Linha::Vazio { texto, nivel } => {
            texto_apagado(recuo(*nivel) + 16.0, texto);
            return None;
        }
        Linha::Erro { texto, nivel, .. } => {
            let r = Rect::from_min_max(pos2(recuo(*nivel), rect.top() + 1.0), pos2(rect.right() - 4.0, rect.bottom() - 1.0));
            pintor.rect_filled(r, CornerRadius::same(tema::RAIO_CONTROLE), tema::fundo_tingido(p, p.erro, tema::claro()));
            pintor.circle_filled(pos2(r.left() + 10.0, r.center().y), 3.5, p.erro);
            registro::texto_cortado(&pintor, pos2(r.left() + 20.0, r.center().y), texto, FontId::proportional(13.0), p.texto, r.width() - 24.0);
            let dica = if *texto == PEDE_SENHA {
                "Clique para digitar a senha".to_string()
            } else if *texto == SENHA_RECUSADA {
                format!("{texto}\nClique para trocar a senha")
            } else {
                format!("{texto}\nClique para tentar de novo")
            };
            let resposta = ui.interact(r, Id::new(("erro-arvore", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(dica);
            return resposta.clicked().then_some(Clique::Escolher);
        }
        Linha::Cortado { mostrando, total, nivel } => {
            let g = pintor.layout_no_wrap(format!("Mostrando {} de {} · ", milhar(*mostrando), milhar(*total)), FontId::proportional(12.5), p.suave);
            let x = recuo(*nivel) + 16.0;
            let w = g.size().x;
            pintor.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.suave);
            pintor.text(pos2(x + w, rect.center().y), Align2::LEFT_CENTER, "filtre para achar", FontId::proportional(12.5), p.destaque);
            return None;
        }
        Linha::MostrarTodos { nivel, con } => {
            let x = recuo(*nivel) + 16.0;
            let g = pintor.layout_no_wrap("Mostrar todos os bancos".into(), FontId::proportional(12.5), p.destaque);
            let link = Rect::from_min_size(pos2(x, rect.center().y - g.size().y / 2.0), g.size());
            pintor.galley(link.min, g, p.destaque);
            let r = ui.interact(link, Id::new(("todos-bancos", con)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            return r.clicked().then_some(Clique::Alternar);
        }
        Linha::Coluna { coluna, nivel } => {
            let x = recuo(*nivel) + 16.0;
            let direita = rect.right() - 8.0;
            // O tipo curto, como na grade ("varchar(200)", "timestamptz"); o
            // nome completo fica na dica. Faltando espaço, o tipo corta antes do nome.
            let curto = tipo_curto(&coluna.tipo);
            let etiquetas: Vec<(&str, egui::Color32, f32)> = [(coluna.pk, "PK", p.destaque), (coluna.fk && !coluna.pk, "FK", p.suave)]
                .into_iter()
                .filter(|(tem, ..)| *tem)
                .map(|(_, r, cor)| (r, cor, pintor.layout_no_wrap(r.into(), tema::fonte_etiqueta(), cor).size().x + 12.0))
                .collect();
            let largura_etiquetas: f32 = etiquetas.iter().map(|(.., w)| w + 6.0).sum();
            let largura_nome = pintor.layout_no_wrap(coluna.nome.clone(), FontId::proportional(12.5), p.texto).size().x;
            let maximo_tipo = (direita - x - largura_nome - largura_etiquetas - 12.0).max(40.0);
            let g_tipo = tema::cortar(&pintor, &curto, egui::TextFormat::simple(FontId::monospace(11.5), p.suave), maximo_tipo, 1, true);
            let mut fim = direita - g_tipo.size().x;
            pintor.galley(pos2(fim, rect.center().y - g_tipo.size().y / 2.0), g_tipo, p.suave);
            for (rotulo, cor, largura) in etiquetas {
                fim -= 6.0 + largura;
                tema::etiqueta(&pintor, pos2(fim, rect.center().y - 9.5), rotulo, tema::fonte_etiqueta(), cor);
            }
            let g = tema::cortar(&pintor, &coluna.nome, egui::TextFormat::simple(FontId::proportional(12.5), p.texto), (fim - x - 8.0).max(20.0), 1, true);
            pintor.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.texto);
            ui.interact(rect, Id::new(("coluna-arvore", i)), Sense::hover()).on_hover_text(format!("{} · {}", coluna.nome, coluna.tipo));
            return None;
        }
        _ => {}
    }
    // Linhas clicáveis.
    let (nivel, aberta, abre, icone, nome, fonte, cor_icone, no): (usize, bool, bool, Option<Icone>, String, FontId, egui::Color32, No) = match linha {
        Linha::Pasta { nome, aberta, .. } => (0, *aberta, true, None, nome.to_string(), forte(13.0), p.suave, No::Pasta(nome.to_string())),
        Linha::Conexao { c, nivel, aberta, erro, .. } => {
            let cor = if erro.is_some() { p.erro } else { p.texto };
            (*nivel, *aberta, true, Some(Icone::Banco), c.nome.clone(), forte(13.0), cor, No::Conexao(c.id))
        }
        Linha::Banco { con, nome, nivel, aberta, .. } => {
            (*nivel, *aberta, true, Some(Icone::Banco), nome.to_string(), FontId::proportional(13.0), p.suave, No::Banco(*con, nome.to_string()))
        }
        Linha::Esquema { con, banco, nome, nivel, aberta, .. } => (
            *nivel,
            *aberta,
            true,
            Some(Icone::Esquema),
            nome.to_string(),
            FontId::proportional(13.0),
            p.suave,
            No::Esquema(*con, banco.to_string(), nome.to_string()),
        ),
        Linha::Grupo { con, banco, esquema, views, nivel, aberta, .. } => (
            *nivel,
            *aberta,
            true,
            None,
            if *views { "Views".into() } else { "Tabelas".into() },
            forte(12.5),
            p.suave,
            No::Grupo(*con, banco.to_string(), esquema.to_string(), *views),
        ),
        Linha::Tabela { con, banco, esquema, nome, view, nivel, aberta, .. } => (
            *nivel,
            *aberta,
            true,
            Some(if *view { Icone::Visao } else { Icone::Tabela }),
            nome.to_string(),
            FontId::proportional(13.0),
            p.suave,
            No::Tabela(*con, banco.to_string(), esquema.to_string(), nome.to_string()),
        ),
        _ => return None,
    };
    let resposta = ui.interact(rect, Id::new(("linha-banco", i, &no)), Sense::click());
    let escolhida = escolhido == Some(&no);
    let fundo = rect.shrink2(vec2(4.0, 1.0));
    if escolhida {
        pintor.rect_filled(fundo, CornerRadius::same(tema::RAIO_ETIQUETA), tema::fundo_escolhido());
        let barra = Rect::from_min_size(fundo.min + vec2(0.0, 3.0), vec2(2.0, fundo.height() - 6.0));
        pintor.rect_filled(barra, CornerRadius::same(1), p.destaque);
    } else if resposta.hovered() {
        pintor.rect_filled(fundo, CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
    }
    let realcada = escolhida || resposta.hovered();
    let x = recuo(nivel);
    if abre {
        seta(&pintor, pos2(x + 6.0, rect.center().y), aberta, if resposta.hovered() { p.texto } else { p.suave });
    }
    let mut x_nome = x + 16.0;
    if let Some(icone) = icone {
        tema::desenhar_icone(&pintor, pos2(x + 16.0 + 8.0, rect.center().y), icone, cor_icone);
        x_nome = x + 16.0 + 16.0 + 6.0;
    }
    // À direita: contagem, selos ou "carregando…".
    let mut direita = rect.right() - 8.0;
    let cor_direita = if realcada { p.texto } else { p.suave };
    let (contagem, carregando, selos): (Option<String>, bool, Selos) = match linha {
        Linha::Pasta { n, .. } => (Some(n.to_string()), false, Vec::new()),
        Linha::Grupo { n, de, .. } => (Some(if !filtro.is_empty() { format!("{n} de {de}") } else { de.to_string() }), false, Vec::new()),
        Linha::Conexao { c, carregando, erro, .. } => {
            let mut selos = Vec::new();
            if erro.is_none() {
                if c.escrita {
                    selos.push((Icone::Lapis, p.alerta, "Alterações permitidas"));
                }
                if c.agentes {
                    selos.push((Icone::Agente, p.suave, "Agentes podem pedir consultas"));
                }
            }
            (None, *carregando, selos)
        }
        Linha::Banco { carregando, .. } | Linha::Esquema { carregando, .. } | Linha::Tabela { carregando, .. } => (None, *carregando, Vec::new()),
        _ => (None, false, Vec::new()),
    };
    if let Linha::Conexao { erro: Some(e), .. } = linha {
        pintor.circle_filled(pos2(direita - 4.0, rect.center().y), 3.5, p.erro);
        direita -= 16.0;
        let _ = e;
    }
    // Selos e contagem terminam no mesmo x (8 px antes do fim da linha).
    for (icone, cor, dica) in selos.iter().rev() {
        let centro = pos2(direita - 7.0, rect.center().y);
        tema::desenhar_icone(&pintor, centro, *icone, *cor);
        ui.interact(Rect::from_center_size(centro, vec2(16.0, 16.0)), Id::new(("selo", i, *dica)), Sense::hover()).on_hover_text(*dica);
        direita -= 20.0;
    }
    if let Some(t) = contagem {
        let g = pintor.layout_no_wrap(t, FontId::proportional(11.5), cor_direita);
        direita -= g.size().x;
        pintor.galley(pos2(direita, rect.center().y - g.size().y / 2.0), g, cor_direita);
        direita -= 8.0;
    }
    let mut largura_nome = (direita - x_nome - 4.0).max(20.0);
    if carregando {
        largura_nome = (largura_nome - 90.0).max(20.0);
    }
    let cor_nome = if matches!(linha, Linha::Grupo { .. }) && !realcada { p.suave } else { p.texto };
    let g = if matches!(linha, Linha::Tabela { .. }) {
        nome_com_filtro(&pintor, &nome, filtro, fonte, largura_nome)
    } else {
        tema::cortar(&pintor, &nome, egui::TextFormat::simple(fonte, cor_nome), largura_nome, 1, true)
    };
    let fim_nome = x_nome + g.size().x;
    pintor.galley(pos2(x_nome, rect.center().y - g.size().y / 2.0), g, cor_nome);
    if carregando {
        let texto = if matches!(linha, Linha::Conexao { .. }) { "conectando…" } else { "carregando…" };
        let formato = egui::TextFormat { font_id: FontId::proportional(12.5), color: p.suave, italics: true, ..Default::default() };
        let g = pintor.layout_job(egui::text::LayoutJob::single_section(texto.into(), formato));
        pintor.galley(pos2(fim_nome + 8.0, rect.center().y - g.size().y / 2.0), g, p.suave);
    }
    let mut resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    if let Linha::Conexao { erro: Some(e), .. } = linha {
        resposta = resposta.on_hover_text(*e);
    }
    let mut clique = None;
    let no_seta = resposta.interact_pointer_pos().is_some_and(|pos| pos.x < x + 14.0);
    if resposta.double_clicked() {
        clique = Some(if matches!(no, No::Tabela(..)) { Clique::Duplo } else { Clique::Alternar });
    } else if resposta.clicked() {
        // Na tabela, o clique simples só escolhe (a seta abre as colunas).
        clique = Some(if matches!(no, No::Tabela(..)) && !no_seta { Clique::Escolher } else { Clique::Alternar });
    }
    let mut menu = None;
    resposta.context_menu(|ui| {
        ui.set_min_width(220.0);
        let item = |ui: &mut egui::Ui, texto: &str, m: Menu, menu: &mut Option<Menu>| {
            if tema::opcao_menu_com(ui, texto, None, true) {
                *menu = Some(m);
                ui.close();
            }
        };
        match no {
            No::Tabela(..) => {
                item(ui, "Primeiras 100 linhas", Menu::Primeiras, &mut menu);
                item(ui, "Gerar SELECT", Menu::Gerar, &mut menu);
                item(ui, "Copiar nome", Menu::CopiarNome, &mut menu);
            }
            No::Pasta(_) => {}
            _ => {
                item(ui, "Abrir console", Menu::Console, &mut menu);
                item(ui, "Editar…", Menu::Editar, &mut menu);
                item(ui, "Duplicar…", Menu::Duplicar, &mut menu);
                item(ui, "Testar", Menu::Testar, &mut menu);
                item(ui, "Atualizar", Menu::Atualizar, &mut menu);
                item(ui, "Trocar senha…", Menu::TrocarSenha, &mut menu);
                item(ui, "Desconectar", Menu::Desconectar, &mut menu);
                ui.separator();
                item(ui, "Remover…", Menu::Remover, &mut menu);
            }
        }
    });
    if let Some(m) = menu {
        clique = Some(Clique::Menu(m));
    }
    clique
}

/// O nome curto do tipo, o mesmo que a grade mostra: "character varying(200)"
/// → "varchar(200)", "timestamp with time zone" → "timestamptz".
pub fn tipo_curto(tipo: &str) -> String {
    let t = tipo.trim();
    let (base, resto) = match t.find('(') {
        Some(i) => (t[..i].trim_end(), &t[i..]),
        None => (t, ""),
    };
    // "timestamp(3) with time zone": o fuso vem depois do parêntese.
    let (args, sufixo) = match resto.find(')') {
        Some(i) => (&resto[..=i], resto[i + 1..].trim()),
        None => (resto, ""),
    };
    let nome = format!("{base}{}", if sufixo.is_empty() { String::new() } else { format!(" {sufixo}") });
    let curto = match nome.to_lowercase().as_str() {
        "character varying" => "varchar",
        "character" => "char",
        "timestamp without time zone" => "timestamp",
        "timestamp with time zone" => "timestamptz",
        "time without time zone" => "time",
        "time with time zone" => "timetz",
        "double precision" => "float8",
        "real" => "float4",
        "integer" => "int4",
        "bigint" => "int8",
        "smallint" => "int2",
        "boolean" => "bool",
        "bit varying" => "varbit",
        _ => return t.to_string(),
    };
    format!("{curto}{args}")
}

/// 20000 → "20.000".
pub fn milhar(n: usize) -> String {
    let s = n.to_string();
    let mut saida = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            saida.push('.');
        }
        saida.push(c);
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conexao(id: i64, tipo: &str, banco: &str, pasta: &str) -> api::ConexaoBanco {
        api::ConexaoBanco { id, nome: format!("c{id}"), tipo: tipo.into(), banco: banco.into(), pasta: pasta.into(), ..Default::default() }
    }

    #[test]
    fn tipos_curtos_como_na_grade() {
        assert_eq!(tipo_curto("character varying(200)"), "varchar(200)");
        assert_eq!(tipo_curto("timestamp with time zone"), "timestamptz");
        assert_eq!(tipo_curto("timestamp(3) without time zone"), "timestamp(3)");
        assert_eq!(tipo_curto("numeric(10,2)"), "numeric(10,2)");
        assert_eq!(tipo_curto("integer"), "int4");
        assert_eq!(tipo_curto("varchar(80)"), "varchar(80)");
        assert_eq!(tipo_curto("int unsigned"), "int unsigned");
    }

    #[test]
    fn erro_de_senha_abre_o_dialogo() {
        assert!(erro_de_senha(PEDE_SENHA));
        assert!(erro_de_senha(SENHA_RECUSADA));
        assert!(!erro_de_senha("Não achou o servidor 127.0.0.1:1."));
    }

    #[test]
    fn milhares_com_ponto() {
        assert_eq!(milhar(7), "7");
        assert_eq!(milhar(1007), "1.007");
        assert_eq!(milhar(23512), "23.512");
    }

    #[test]
    fn abrir_pede_o_nivel_e_um_filho_so_abre_sozinho() {
        let mut a = Arvore::default();
        let mut acoes = Vec::new();
        a.alternar(No::Conexao(1), "postgres", "", &mut acoes);
        assert_eq!(acoes, vec![Acao::Pedir(Pedido::Bancos(1))]);
        acoes.clear();
        a.receber(Pedido::Bancos(1), Ok(Resposta::Bancos(api::Nomes { nomes: vec!["loja".into()] })), "postgres", &mut acoes);
        assert_eq!(acoes, vec![Acao::Pedir(Pedido::Esquemas(1, "loja".into()))]);
        acoes.clear();
        a.receber(Pedido::Esquemas(1, "loja".into()), Ok(Resposta::Nomes(vec!["public".into()])), "postgres", &mut acoes);
        assert_eq!(acoes, vec![Acao::Pedir(Pedido::Objetos(1, "loja".into(), "public".into()))]);
        let tabelas: Vec<String> = (1..=1007).map(|i| format!("t{i:04}")).collect();
        a.receber(
            Pedido::Objetos(1, "loja".into(), "public".into()),
            Ok(Resposta::Objetos(api::Objetos { tabelas, views: vec![], total: 1007, cortado: false })),
            "postgres",
            &mut acoes,
        );
        let conexoes = [conexao(1, "postgres", "", "")];
        let linhas = a.linhas(&conexoes);
        // conexão, banco, esquema, grupo Tabelas (aberto: só tabelas) e as 1007.
        assert_eq!(linhas.len(), 4 + 1007);
        assert!(matches!(linhas[3], Linha::Grupo { n: 1007, de: 1007, aberta: true, .. }));
        a.filtro = "T000".into();
        let linhas = a.linhas(&conexoes);
        assert!(matches!(linhas[3], Linha::Grupo { n: 9, de: 1007, .. }));
        assert_eq!(linhas.len(), 4 + 9);
    }

    #[test]
    fn banco_padrao_comeca_so_nele_e_sqlite_vai_direto_as_tabelas() {
        let mut a = Arvore::default();
        let mut acoes = Vec::new();
        a.alternar(No::Conexao(1), "mysql", "loja", &mut acoes);
        assert_eq!(acoes, vec![Acao::Pedir(Pedido::Objetos(1, "loja".into(), String::new()))]);
        let conexoes = [conexao(1, "mysql", "loja", "Trabalho"), conexao(2, "sqlite", "", "Trabalho")];
        let linhas = a.linhas(&conexoes);
        assert!(matches!(linhas[0], Linha::Pasta { n: 2, .. }));
        assert!(matches!(linhas.last().unwrap(), Linha::Conexao { .. }));
        assert!(linhas.iter().any(|l| matches!(l, Linha::MostrarTodos { con: 1, .. })));
        acoes.clear();
        a.alternar(No::Conexao(2), "sqlite", "", &mut acoes);
        assert_eq!(acoes, vec![Acao::Pedir(Pedido::Objetos(2, String::new(), String::new()))]);
    }
}
