//! A tela de bancos de dados do perfil: a árvore das conexões à esquerda e o
//! console da conexão escolhida à direita (editor em cima, resultado ou
//! histórico embaixo). Tudo o que fala com o núcleo roda numa thread e
//! acorda a tela quando volta: nada de consulta periódica; só a execução em
//! andamento redesenha a cada segundo, para o contador.

pub mod aprovacao;
mod arvore;
mod conexao;
mod console;
mod sql;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, CornerRadius, Key, Modifiers, Rect, RichText, Stroke, pos2, vec2};

use crate::api;
use crate::registro;
use crate::tema::{self, TipoAviso, cores};

/// O que a tela de bancos pede à tela principal.
pub enum Acao {
    AbrirTarefa { tarefa: i64, agente: i64 },
    Avisar(TipoAviso, String),
}

enum Resposta {
    Lista(Result<api::ListaConexoes, String>),
    Arvore(arvore::Pedido, Result<arvore::Resposta, api::ErroBanco>),
    Executou {
        conexao: i64,
        ficha: String,
        banco: String,
        resultado: Result<api::Resultado, api::ErroBanco>,
    },
    Mais {
        conexao: i64,
        resultado: Result<api::Resultado, api::ErroBanco>,
    },
    /// `tabela` é o (banco, esquema, tabela) pedido, para refazer a prévia.
    Previa {
        conexao: i64,
        nome: String,
        banco: String,
        tabela: (String, String, String),
        resultado: Result<api::PreviaTabela, api::ErroBanco>,
    },
    Historico(i64, Result<Vec<api::ConsultaFeita>, String>),
    Testado(Result<api::Teste, String>),
    TestouConexao(i64, String, Result<api::Teste, api::ErroBanco>),
    Salvou(Result<api::ConexaoBanco, String>),
    SenhaDefinida(i64, Result<String, String>),
}

/// O que refazer quando a senha chegar.
enum Pendente {
    /// Um nível da árvore (a senha aceita recarrega os abertos).
    Arvore,
    Executar {
        conexao: i64,
        sql: String,
        banco: String,
        confirmar: String,
    },
    Previa(i64, String, String, String),
    Testar(i64),
}

pub struct Bancos {
    perfil: i64,
    pub nome_perfil: String,
    pub conexoes: Vec<api::ConexaoBanco>,
    pub chaveiro: bool,
    carregou: bool,
    pedindo_lista: bool,
    /// Chegou outro motivo para ler a lista enquanto um pedido estava no
    /// ar: ao voltar, pede de novo (a resposta mais nova nunca se perde).
    lista_de_novo: bool,
    reler_ao_mostrar: bool,
    erro_lista: Option<String>,
    escolhida: Option<i64>,
    arvore: arvore::Arvore,
    consoles: HashMap<i64, console::Console>,
    dialogo: Option<conexao::Dialogo>,
    senha: Option<conexao::PedirSenha>,
    pendente: Option<Pendente>,
    confirmar: Option<conexao::ConfirmarEscrita>,
    remover: Option<(i64, String, Option<String>)>,
    limpar: Option<(i64, String, Option<String>)>,
    largura_arvore: f32,
    fracao_editor: f32,
    envio: Sender<Resposta>,
    recebe: Receiver<Resposta>,
    acoes: Vec<Acao>,
}

impl Bancos {
    pub fn novo(perfil: i64) -> Bancos {
        let (envio, recebe) = mpsc::channel();
        Bancos {
            perfil,
            nome_perfil: String::new(),
            conexoes: Vec::new(),
            chaveiro: false,
            carregou: false,
            pedindo_lista: false,
            lista_de_novo: false,
            reler_ao_mostrar: false,
            erro_lista: None,
            escolhida: None,
            arvore: arvore::Arvore::default(),
            consoles: HashMap::new(),
            dialogo: None,
            senha: None,
            pendente: None,
            confirmar: None,
            remover: None,
            limpar: None,
            largura_arvore: 300.0,
            fracao_editor: 0.4,
            envio,
            recebe,
            acoes: Vec::new(),
        }
    }

    /// Um diálogo dos bancos está aberto (os atalhos da Colmeia esperam).
    pub fn modal_aberto(&self) -> bool {
        self.dialogo.is_some() || self.senha.is_some() || self.confirmar.is_some() || self.remover.is_some() || self.limpar.is_some()
    }

    pub fn nova_conexao(&mut self, tipo: &str) {
        self.dialogo = Some(conexao::Dialogo::novo(tipo).com_nomes(self.nomes()));
    }

    /// O nome da conexão do console aberto (vai na trilha do cabeçalho).
    pub fn nome_escolhida(&self) -> Option<String> {
        self.escolhida.and_then(|id| self.conexao(id)).map(|c| c.nome.clone())
    }

    fn nomes(&self) -> Vec<String> {
        self.conexoes.iter().map(|c| c.nome.clone()).collect()
    }

    /// A tela de bancos apareceu: a lista é lida de novo no próximo quadro
    /// (uma conexão criada fora dela, ou um aviso perdido, aparece).
    pub fn marcar_aberta(&mut self) {
        self.reler_ao_mostrar = true;
    }

    /// Abrir a tela já numa conexão (a linha do tempo).
    pub fn abrir_conexao(&mut self, id: i64) {
        self.escolhida = Some(id);
        self.arvore.escolhido = Some(arvore::No::Conexao(id));
    }

    fn em_segundo_plano(&self, ctx: &egui::Context, pedido: impl FnOnce() -> Resposta + Send + 'static) {
        registro::em_segundo_plano(&self.envio, ctx, pedido);
    }

    /// Lê a lista de conexões de novo (mudou no núcleo).
    pub fn recarregar(&mut self, ctx: &egui::Context) {
        if self.perfil == 0 {
            return;
        }
        if self.pedindo_lista {
            self.lista_de_novo = true;
            return;
        }
        self.pedindo_lista = true;
        let perfil = self.perfil;
        self.em_segundo_plano(ctx, move || Resposta::Lista(api::conexoes(perfil)));
    }

    /// Houve consulta numa conexão: o histórico dela é lido de novo quando
    /// aparecer (ou já, se estiver na tela).
    pub fn houve_consulta(&mut self, ctx: &egui::Context, conexao: i64) {
        let mut ler = false;
        if let Some(c) = self.consoles.get_mut(&conexao) {
            if c.aba_historico && c.historico.is_some() {
                ler = true;
            } else {
                c.historico = None;
            }
        }
        if ler {
            self.ler_historico(ctx, conexao);
        }
    }

    fn conexao(&self, id: i64) -> Option<&api::ConexaoBanco> {
        self.conexoes.iter().find(|c| c.id == id)
    }

    fn console(&mut self, id: i64) -> &mut console::Console {
        let padrao = self.conexoes.iter().find(|c| c.id == id).map(|c| c.banco.clone()).unwrap_or_default();
        self.consoles.entry(id).or_insert_with(|| console::Console::novo(&padrao))
    }

    fn ler_historico(&mut self, ctx: &egui::Context, conexao: i64) {
        self.em_segundo_plano(ctx, move || Resposta::Historico(conexao, api::historico(conexao)));
    }

    fn pedir_arvore(&self, ctx: &egui::Context, pedido: arvore::Pedido) {
        self.em_segundo_plano(ctx, move || {
            let resposta = match &pedido {
                arvore::Pedido::Bancos(c) => api::bancos_do_servidor(*c).map(arvore::Resposta::Bancos),
                arvore::Pedido::Esquemas(c, b) => api::esquemas(*c, b).map(|n| arvore::Resposta::Nomes(n.nomes)),
                arvore::Pedido::Objetos(c, b, e) => api::objetos(*c, b, e).map(arvore::Resposta::Objetos),
                arvore::Pedido::Colunas(c, b, e, t) => api::colunas_da_tabela(*c, b, e, t).map(arvore::Resposta::Colunas),
            };
            Resposta::Arvore(pedido, resposta)
        });
    }

    fn pedir_senha(&mut self, ctx: &egui::Context, conexao: i64, depois: Pendente) {
        let nome = self.conexao(conexao).map(|c| c.nome.clone()).unwrap_or_default();
        if self.senha.as_ref().is_none_or(|s| s.conexao != conexao) {
            self.senha = Some(conexao::PedirSenha::nova(conexao, &nome));
            // O chaveiro pode ter sumido ou voltado (núcleo reiniciado): a
            // lista traz o estado dele, e o diálogo acompanha.
            self.recarregar(ctx);
        }
        self.pendente = Some(depois);
    }

    fn executar(&mut self, ctx: &egui::Context, conexao: i64, sql: String, banco: String, confirmar: String, agora: f64) {
        let c = self.console(conexao);
        let ficha = c.nova_ficha(conexao);
        let (limite, tempo_s) = (c.limite, c.tempo_s);
        c.executando = Some(console::Rodando { ficha: ficha.clone(), desde: agora, mais: false });
        self.em_segundo_plano(ctx, move || {
            let resultado = api::executar(conexao, &api::Execucao { ficha: &ficha, sql: &sql, banco: &banco, limite, tempo_s, confirmar: &confirmar });
            Resposta::Executou { conexao, ficha, banco, resultado }
        });
    }

    fn receber(&mut self, ctx: &egui::Context, agora: f64) {
        let respostas: Vec<Resposta> = self.recebe.try_iter().collect();
        for r in respostas {
            match r {
                Resposta::Lista(Ok(l)) => {
                    self.pedindo_lista = false;
                    if std::mem::take(&mut self.lista_de_novo) {
                        self.recarregar(ctx);
                    }
                    self.carregou = true;
                    self.erro_lista = None;
                    self.chaveiro = l.chaveiro_disponivel;
                    // A conexão editada pode ter mudado de banco padrão ou de senha.
                    for nova in &l.conexoes {
                        if let Some(velha) = self.conexoes.iter().find(|c| c.id == nova.id)
                            && mudou_o_destino(velha, nova)
                        {
                            self.arvore.esquecer(nova.id);
                            self.arvore.abertos.retain(|n| n.conexao() != nova.id);
                        }
                    }
                    self.conexoes = l.conexoes;
                    self.consoles.retain(|id, _| self.conexoes.iter().any(|c| c.id == *id));
                    if self.escolhida.is_none_or(|e| !self.conexoes.iter().any(|c| c.id == e)) {
                        self.escolhida = self.conexoes.first().map(|c| c.id);
                    }
                }
                Resposta::Lista(Err(e)) => {
                    self.pedindo_lista = false;
                    if std::mem::take(&mut self.lista_de_novo) {
                        self.recarregar(ctx);
                    }
                    self.carregou = true;
                    self.erro_lista = Some(e);
                }
                Resposta::Arvore(pedido, resultado) => {
                    let con = match &pedido {
                        arvore::Pedido::Bancos(c) | arvore::Pedido::Esquemas(c, _) | arvore::Pedido::Objetos(c, ..) | arvore::Pedido::Colunas(c, ..) => *c,
                    };
                    let tipo = self.conexao(con).map(|c| c.tipo.clone()).unwrap_or_default();
                    let resultado = match resultado {
                        Err(api::ErroBanco::Senha) => {
                            self.pedir_senha(ctx, con, Pendente::Arvore);
                            Err(arvore::PEDE_SENHA.to_string())
                        }
                        // A frase fixa faz a linha do erro abrir o diálogo de senha.
                        Err(api::ErroBanco::Falhou(f)) if f.senha_recusada => Err(arvore::SENHA_RECUSADA.to_string()),
                        Err(e) => Err(e.texto()),
                        Ok(r) => Ok(r),
                    };
                    let mut acoes = Vec::new();
                    self.arvore.receber(pedido, resultado, &tipo, &mut acoes);
                    self.tratar_arvore(ctx, acoes, agora);
                }
                Resposta::Executou { conexao, ficha, banco, resultado } => {
                    let nome = self.conexao(conexao).map(|c| c.nome.clone()).unwrap_or_default();
                    let tipo = self.conexao(conexao).map(|c| c.tipo.clone()).unwrap_or_default();
                    let c = self.console(conexao);
                    if c.executando.as_ref().is_some_and(|e| e.ficha == ficha) {
                        c.executando = None;
                    }
                    c.historico = None;
                    c.previa_falhou = None;
                    match resultado {
                        Ok(r) => c.resultado_novo(r, ficha, banco, None),
                        Err(api::ErroBanco::Confirmar(confirmacao)) => {
                            self.confirmar = Some(conexao::ConfirmarEscrita::nova(conexao, &nome, &tipo, confirmacao))
                        }
                        Err(api::ErroBanco::Senha) => {
                            let sql = c.trecho(ctx, &tipo).map(|t| t.1).unwrap_or_default();
                            self.pedir_senha(ctx, conexao, Pendente::Executar { conexao, sql, banco, confirmar: String::new() });
                        }
                        Err(api::ErroBanco::Falhou(f)) => {
                            c.aba_historico = false;
                            c.painel = console::Painel::Falhou(f);
                        }
                    }
                }
                Resposta::Mais { conexao, resultado } => {
                    let c = self.console(conexao);
                    c.executando = None;
                    match resultado {
                        Ok(r) => c.mais_linhas(r),
                        Err(api::ErroBanco::Falhou(f)) if f.fechada => {
                            if let console::Painel::Resultado { fechada, .. } = &mut c.painel {
                                *fechada = true;
                            }
                        }
                        Err(e) => self.acoes.push(Acao::Avisar(TipoAviso::Erro, e.texto())),
                    }
                }
                Resposta::Previa { conexao, nome, banco, tabela, resultado } => {
                    let c = self.console(conexao);
                    c.executando = None;
                    c.previa_falhou = None;
                    match resultado {
                        Ok(p) => {
                            let ficha = c.nova_ficha(conexao);
                            c.resultado_novo(p.resultado, ficha, banco, Some(nome));
                        }
                        Err(api::ErroBanco::Senha) => {
                            let (b, e, t) = tabela;
                            self.pedir_senha(ctx, conexao, Pendente::Previa(conexao, b, e, t));
                        }
                        Err(e) => {
                            // "Trocar senha…" neste erro refaz a prévia, não a instrução do editor.
                            c.previa_falhou = Some(tabela);
                            c.painel = console::Painel::Falhou(match e {
                                api::ErroBanco::Falhou(f) => f,
                                outro => api::FalhaBanco { erro: outro.texto(), ..Default::default() },
                            })
                        }
                    }
                }
                Resposta::Historico(conexao, h) => {
                    if let Some(c) = self.consoles.get_mut(&conexao) {
                        c.historico = Some(h);
                    }
                }
                Resposta::Testado(t) => {
                    if let Some(d) = &mut self.dialogo {
                        d.testando = false;
                        d.teste = Some(t);
                    }
                }
                Resposta::TestouConexao(id, nome, t) => match t {
                    Ok(t) if t.ok => {
                        let tls = if t.tls { "com TLS" } else { "sem TLS" };
                        self.acoes.push(Acao::Avisar(TipoAviso::Neutro, format!("{nome}: conectou em {} ms · {} · {tls}", t.ms, t.servidor)));
                    }
                    Ok(t) => self.acoes.push(Acao::Avisar(TipoAviso::Erro, format!("{nome}: {}", t.erro))),
                    Err(api::ErroBanco::Senha) => self.pedir_senha(ctx, id, Pendente::Testar(id)),
                    Err(e) => self.acoes.push(Acao::Avisar(TipoAviso::Erro, format!("{nome}: {}", e.texto()))),
                },
                Resposta::Salvou(r) => match r {
                    Ok(c) => {
                        let id = c.id;
                        self.dialogo = None;
                        match self.conexoes.iter_mut().find(|x| x.id == id) {
                            // Mudou host, porta, usuário, banco ou arquivo: a árvore
                            // recomeça fechada (o cache era de outro lugar).
                            Some(velha) if mudou_o_destino(velha, &c) => {
                                self.arvore.esquecer(id);
                                self.arvore.abertos.retain(|n| n.conexao() != id);
                                *velha = c.clone();
                            }
                            // Só nome, pasta, escrita, agentes…: nós abertos e escolha ficam.
                            Some(velha) => *velha = c.clone(),
                            None => self.conexoes.push(c.clone()),
                        }
                        // Ligou a escrita: o aviso "somente leitura" do resultado já não vale.
                        if c.escrita
                            && let Some(con) = self.consoles.get_mut(&id)
                            && matches!(&con.painel, console::Painel::Falhou(f) if f.somente_leitura)
                        {
                            con.painel = console::Painel::Nada;
                        }
                        if self.arvore.escolhido.as_ref().is_none_or(|n| n.conexao() != id) {
                            self.arvore.escolhido = Some(arvore::No::Conexao(id));
                        }
                        self.escolhida = Some(id);
                        self.recarregar(ctx);
                    }
                    Err(e) => {
                        if let Some(d) = &mut self.dialogo {
                            d.salvando = false;
                            d.erro = Some(e);
                        }
                    }
                },
                Resposta::SenhaDefinida(conexao, r) => match r {
                    Ok(_) => {
                        self.senha = None;
                        self.recarregar(ctx);
                        // Senha aceita: o erro sai da árvore e os níveis abertos
                        // da conexão carregam de novo, sem precisar de "Atualizar".
                        let (tipo, padrao) = self.conexao(conexao).map(|c| (c.tipo.clone(), c.banco.clone())).unwrap_or_default();
                        let mut pedidos = Vec::new();
                        self.arvore.atualizar(conexao, &tipo, &padrao, &mut pedidos);
                        self.tratar_arvore(ctx, pedidos, agora);
                        // O erro "senha recusada" do console já não vale.
                        if let Some(con) = self.consoles.get_mut(&conexao)
                            && matches!(&con.painel, console::Painel::Falhou(f) if f.senha_recusada)
                        {
                            con.painel = console::Painel::Nada;
                        }
                        match self.pendente.take() {
                            Some(Pendente::Arvore) => {}
                            Some(Pendente::Executar { conexao, sql, banco, confirmar }) if !sql.is_empty() => {
                                self.executar(ctx, conexao, sql, banco, confirmar, agora);
                            }
                            Some(Pendente::Previa(c, b, e, t)) => self.previa(ctx, c, b, e, t),
                            Some(Pendente::Testar(c)) => self.testar(ctx, c),
                            _ => {}
                        }
                    }
                    Err(e) => {
                        if let Some(s) = &mut self.senha {
                            s.enviando = false;
                            s.erro = Some(e);
                        }
                    }
                },
            }
        }
    }

    fn previa(&mut self, ctx: &egui::Context, c: i64, b: String, e: String, t: String) {
        let nome = [b.as_str(), e.as_str(), t.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(".");
        let nome = if e.is_empty() { nome } else { format!("{e}.{t}") };
        self.escolhida = Some(c);
        let con = self.console(c);
        con.executando = Some(console::Rodando { ficha: String::new(), desde: ctx.input(|i| i.time), mais: false });
        let banco = b.clone();
        self.em_segundo_plano(ctx, move || {
            let resultado = api::previa_tabela(c, &b, &e, &t);
            Resposta::Previa { conexao: c, nome, banco, tabela: (b, e, t), resultado }
        });
    }

    fn testar(&mut self, ctx: &egui::Context, c: i64) {
        let nome = self.conexao(c).map(|c| c.nome.clone()).unwrap_or_default();
        let envio = self.envio.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let t = api::testar_conexao(c);
            let _ = envio.send(Resposta::TestouConexao(c, nome, t));
            ctx2.request_repaint();
        });
    }

    fn tratar_arvore(&mut self, ctx: &egui::Context, acoes: Vec<arvore::Acao>, _agora: f64) {
        for a in acoes {
            match a {
                arvore::Acao::Pedir(p) => self.pedir_arvore(ctx, p),
                arvore::Acao::Escolher(c) => {
                    self.escolhida = Some(c);
                    self.console(c);
                }
                arvore::Acao::Previa(c, b, e, t) => self.previa(ctx, c, b, e, t),
                arvore::Acao::GerarSelect(c, _b, e, t) => {
                    let tipo = self.conexao(c).map(|x| x.tipo.clone()).unwrap_or_default();
                    self.escolhida = Some(c);
                    let sql = sql_gerado(&tipo, &e, &t, 100);
                    self.console(c).inserir(ctx, &sql);
                }
                arvore::Acao::CopiarNome(n) => ctx.copy_text(n),
                arvore::Acao::Editar(c) => {
                    if let Some(c) = self.conexao(c) {
                        self.dialogo = Some(conexao::Dialogo::editar(c));
                    }
                }
                arvore::Acao::Duplicar(c) => {
                    if let Some(c) = self.conexao(c) {
                        self.dialogo = Some(conexao::Dialogo::duplicar(c));
                    }
                }
                arvore::Acao::Testar(c) => self.testar(ctx, c),
                arvore::Acao::Atualizar(_) => {}
                arvore::Acao::Desconectar(c) => {
                    std::thread::spawn(move || {
                        let _ = api::desconectar(c);
                    });
                }
                arvore::Acao::Remover(c) => {
                    if let Some(x) = self.conexao(c) {
                        self.remover = Some((c, x.nome.clone(), None));
                    }
                }
                arvore::Acao::TrocarSenha(c) => {
                    let nome = self.conexao(c).map(|c| c.nome.clone()).unwrap_or_default();
                    self.senha = Some(conexao::PedirSenha::nova(c, &nome));
                    self.pendente = None;
                    self.recarregar(ctx);
                }
            }
        }
    }

    /// A tela inteira. `aprovacoes`: os pedidos dos agentes esperando você.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, aprovacoes: &[api::Aprovacao], nomes: &dyn Fn(i64) -> String, agora: f64) -> Vec<Acao> {
        let ctx = ui.ctx().clone();
        self.receber(&ctx, agora);
        if !self.carregou || std::mem::take(&mut self.reler_ao_mostrar) {
            self.recarregar(&ctx);
        }
        let p = cores();
        // A faixa dos pedidos de consulta.
        if let Some(primeiro) = aprovacoes.first() {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_CONTROLE), tema::fundo_tingido(p, p.alerta, tema::claro()));
            ui.painter().circle_filled(pos2(rect.left() + 14.0, rect.center().y), 3.5, p.alerta);
            let n = aprovacoes.len();
            let quem = if primeiro.agente.is_empty() { "O agente" } else { primeiro.agente.as_str() };
            let texto = if n == 1 {
                format!("1 pedido de consulta: {quem} quer consultar {}", primeiro.conexao)
            } else {
                format!("{n} pedidos de consulta: {quem} quer consultar {} e mais", primeiro.conexao)
            };
            ui.painter().text(pos2(rect.left() + 28.0, rect.center().y), egui::Align2::LEFT_CENTER, texto, egui::FontId::proportional(13.0), p.texto);
            let botao = Rect::from_min_max(pos2(rect.right() - 70.0, rect.top() + 2.0), pos2(rect.right() - 6.0, rect.bottom() - 2.0));
            if ui.put(botao, |ui: &mut egui::Ui| tema::botao_secundario(ui, "Ver")).clicked() {
                self.acoes.push(Acao::AbrirTarefa { tarefa: primeiro.tarefa_id, agente: primeiro.agente_id });
            }
            ui.add_space(8.0);
        }
        if self.carregou && self.conexoes.is_empty() {
            self.vazio(ui);
        } else if let Some(e) = &self.erro_lista {
            ui.label(RichText::new(format!("Não consegui ler as conexões: {e}")).color(p.erro));
        } else {
            self.corpo(ui, nomes, agora);
        }
        self.dialogos(&ctx, agora);
        if self.consoles.values().any(|c| c.executando.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        std::mem::take(&mut self.acoes)
    }

    /// Estado vazio: os quatro tipos em ladrilhos.
    fn vazio(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.18);
            ui.allocate_ui(vec2(560.0, 0.0), |ui| {
                egui::Frame::new()
                    .fill(p.superficie_alta)
                    .stroke(Stroke::new(1.0, p.borda))
                    .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
                    .inner_margin(egui::Margin::same(24))
                    .show(ui, |ui| {
                        ui.set_width(512.0);
                        tema::cabecalho(
                            ui,
                            &format!("Conecte um banco ao perfil {}", self.nome_perfil),
                            "A senha fica no chaveiro do sistema. Toda conexão começa só leitura.",
                        );
                        ui.add_space(20.0);
                        if let Some(tipo) = conexao::ladrilhos(ui) {
                            self.nova_conexao(tipo);
                        }
                    });
            });
        });
    }

    fn corpo(&mut self, ui: &mut egui::Ui, nomes: &dyn Fn(i64) -> String, agora: f64) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let area = ui.available_rect_before_wrap();
        let maxima = (area.width() * 0.45).max(220.0);
        self.largura_arvore = self.largura_arvore.clamp(220.0, maxima);
        let r_arvore = Rect::from_min_size(area.min, vec2(self.largura_arvore, area.height()));
        let r_divisa = Rect::from_min_size(pos2(r_arvore.right(), area.top()), vec2(8.0, area.height()));
        let r_console = Rect::from_min_max(pos2(r_divisa.right(), area.top()), area.max);
        // Ctrl+F: o filtro da árvore (fora do editor).
        let editor_com_foco = ctx.memory(|m| m.has_focus(console::id_editor()));
        if !editor_com_foco && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
            self.arvore.focar_filtro = true;
        }
        // A árvore.
        ui.painter().rect_filled(r_arvore, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie);
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(r_arvore.shrink(8.0)));
        let acoes = self.arvore.mostrar(&mut filho, &self.conexoes);
        self.tratar_arvore(&ctx, acoes, agora);
        self.largura_arvore += tema::divisoria(ui, r_divisa, egui::Id::new("divisa-arvore"), true);
        // O console da conexão escolhida.
        let Some(id) = self.escolhida.filter(|e| self.conexoes.iter().any(|c| c.id == *e)) else {
            ui.painter().text(r_console.center(), egui::Align2::CENTER_CENTER, "Escolha uma conexão na árvore.", egui::FontId::proportional(13.0), p.suave);
            return;
        };
        let conexao = self.conexao(id).cloned().unwrap_or_default();
        let bancos: Vec<String> = match self.arvore.bancos.get(&id) {
            Some(arvore::Carga::Pronta(n)) => n.nomes.clone(),
            _ => Vec::new(),
        };
        let mut acoes = Vec::new();
        let barra = Rect::from_min_size(r_console.min, vec2(r_console.width(), 34.0));
        let resto = Rect::from_min_max(pos2(r_console.left(), barra.bottom() + 8.0), r_console.max);
        let altura_editor = ((resto.height() - 8.0) * self.fracao_editor).clamp(120.0, (resto.height() - 8.0 - 160.0).max(120.0));
        let r_editor = Rect::from_min_size(resto.min, vec2(resto.width(), altura_editor));
        let r_divisa2 = Rect::from_min_size(pos2(resto.left(), r_editor.bottom()), vec2(resto.width(), 8.0));
        let r_painel = Rect::from_min_max(pos2(resto.left(), r_divisa2.bottom()), resto.max);
        {
            let c = self.console(id);
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(barra));
            c.barra(&mut filho, &conexao, &bancos, agora, &mut acoes);
            c.editor(ui, r_editor, &conexao.tipo, agora, &mut acoes);
        }
        let d = tema::divisoria(ui, r_divisa2, egui::Id::new("divisa-editor"), false);
        if d != 0.0 && resto.height() > 0.0 {
            self.fracao_editor = ((altura_editor + d) / (resto.height() - 8.0)).clamp(0.15, 0.85);
        }
        let padrao = conexao.banco.clone();
        self.console(id).painel(ui, r_painel, &conexao.tipo, &padrao, nomes, &mut acoes);
        for a in acoes {
            match a {
                console::Acao::Executar => {
                    let c = self.console(id);
                    if c.executando.is_some() {
                        continue;
                    }
                    match c.trecho(&ctx, &conexao.tipo) {
                        Ok((faixa, sql)) => {
                            c.marcado = Some((faixa, agora + 0.6));
                            let banco = c.banco.clone();
                            self.executar(&ctx, id, sql, banco, String::new(), agora);
                        }
                        Err(texto) => {
                            c.aba_historico = false;
                            c.painel = console::Painel::Info(texto);
                        }
                    }
                }
                console::Acao::Cancelar => {
                    if let Some(r) = &self.console(id).executando
                        && !r.ficha.is_empty()
                    {
                        let ficha = r.ficha.clone();
                        std::thread::spawn(move || {
                            let _ = api::cancelar_execucao(&ficha);
                        });
                    }
                }
                console::Acao::Mais => {
                    let c = self.console(id);
                    if let console::Painel::Resultado { ficha, .. } = &c.painel {
                        let ficha = ficha.clone();
                        let (limite, tempo_s) = (c.limite, c.tempo_s);
                        c.executando = Some(console::Rodando { ficha: ficha.clone(), desde: agora, mais: true });
                        self.em_segundo_plano(&ctx, move || Resposta::Mais { conexao: id, resultado: api::carregar_mais(&ficha, limite, tempo_s) });
                    }
                }
                console::Acao::EditarConexao => {
                    self.dialogo = Some(conexao::Dialogo::editar(&conexao));
                }
                console::Acao::TrocarSenha => {
                    // Com a senha nova aceita, o que falhou roda de novo: a prévia
                    // da tabela, ou a instrução sob o cursor.
                    self.pendente = Some(match self.console(id).previa_falhou.clone() {
                        Some((b, e, t)) => Pendente::Previa(id, b, e, t),
                        None => {
                            let sql = self.console(id).trecho(&ctx, &conexao.tipo).map(|t| t.1).unwrap_or_default();
                            let banco = self.console(id).banco.clone();
                            Pendente::Executar { conexao: id, sql, banco, confirmar: String::new() }
                        }
                    });
                    self.senha = Some(conexao::PedirSenha::nova(id, &conexao.nome));
                    self.recarregar(&ctx);
                }
                console::Acao::LerHistorico => self.ler_historico(&ctx, id),
                console::Acao::LimparHistorico => self.limpar = Some((id, conexao.nome.clone(), None)),
                console::Acao::Avisar(t) => self.acoes.push(Acao::Avisar(TipoAviso::Neutro, t)),
            }
        }
    }

    fn dialogos(&mut self, ctx: &egui::Context, agora: f64) {
        let pastas: Vec<String> = {
            let mut v: Vec<String> = self.conexoes.iter().filter(|c| !c.pasta.is_empty()).map(|c| c.pasta.clone()).collect();
            v.sort();
            v.dedup();
            v
        };
        if let Some(d) = &mut self.dialogo {
            match d.mostrar(ctx, self.chaveiro, &pastas) {
                conexao::Pedido::Continua => {}
                conexao::Pedido::Fechar => self.dialogo = None,
                conexao::Pedido::Testar => {
                    d.testando = true;
                    d.teste = None;
                    let (perfil, campos, senha, id) = (self.perfil, d.campos.clone(), d.senha.clone(), d.editando.as_ref().map_or(0, |c| c.id));
                    self.em_segundo_plano(ctx, move || Resposta::Testado(api::testar_rascunho(perfil, &campos, &senha, id)));
                }
                conexao::Pedido::Salvar => {
                    d.salvando = true;
                    d.erro = None;
                    let (perfil, campos, guardar) = (self.perfil, d.campos.clone(), d.guardar && self.chaveiro);
                    let senha = (!d.senha.is_empty()).then(|| d.senha.clone());
                    let editando = d.editando.as_ref().map(|c| c.id);
                    self.em_segundo_plano(ctx, move || {
                        Resposta::Salvou(match editando {
                            Some(id) => api::editar_conexao(id, &campos, senha.as_deref(), guardar),
                            None => api::criar_conexao(perfil, &campos, senha.as_deref(), guardar),
                        })
                    });
                }
                conexao::Pedido::EsquecerSenha(id) => {
                    if let Err(e) = api::esquecer_senha(id) {
                        d.erro = Some(e);
                    } else if let Some(c) = &mut d.editando {
                        c.senha = "memoria".into();
                        self.recarregar(ctx);
                    }
                }
            }
        }
        if let Some(s) = &mut self.senha {
            match s.mostrar(ctx, self.chaveiro) {
                conexao::RespostaSenha::Continua => {}
                conexao::RespostaSenha::Cancelar => {
                    self.senha = None;
                    self.pendente = None;
                }
                conexao::RespostaSenha::Conectar => {
                    s.enviando = true;
                    s.erro = None;
                    let (id, senha, guardar) = (s.conexao, s.senha.clone(), s.guardar && self.chaveiro);
                    self.em_segundo_plano(ctx, move || Resposta::SenhaDefinida(id, api::definir_senha(id, &senha, guardar)));
                }
            }
        }
        if let Some(c) = &mut self.confirmar {
            match c.mostrar(ctx) {
                conexao::RespostaEscrita::Continua => {}
                conexao::RespostaEscrita::Cancelar => self.confirmar = None,
                conexao::RespostaEscrita::Executar => {
                    let (id, sql, banco, nonce) = (c.conexao, c.c.sql.clone(), c.c.banco.clone(), c.c.confirmacao.clone());
                    self.confirmar = None;
                    self.executar(ctx, id, sql, banco, nonce, agora);
                }
            }
        }
        if let Some((id, nome, erro)) = &mut self.remover {
            let texto = format!("Remover {nome}? A senha sai do chaveiro e o histórico de consultas é apagado. A linha do tempo continua.");
            match conexao::confirmar(ctx, "remover-conexao", "Remover conexão", &texto, "Remover", erro) {
                Some(true) => match api::remover_conexao(*id) {
                    Ok(()) => {
                        let id = *id;
                        self.remover = None;
                        self.consoles.remove(&id);
                        self.arvore.esquecer(id);
                        self.recarregar(ctx);
                    }
                    Err(e) => *erro = Some(e),
                },
                Some(false) => self.remover = None,
                None => {}
            }
        }
        if let Some((id, nome, erro)) = &mut self.limpar {
            let texto = format!("Apagar o histórico de consultas de {nome}? A linha do tempo continua.");
            match conexao::confirmar(ctx, "limpar-historico-banco", "Limpar histórico", &texto, "Limpar", erro) {
                Some(true) => match api::limpar_historico(*id) {
                    Ok(()) => {
                        let id = *id;
                        self.limpar = None;
                        if let Some(c) = self.consoles.get_mut(&id) {
                            c.historico = Some(Ok(Vec::new()));
                        }
                    }
                    Err(e) => *erro = Some(e),
                },
                Some(false) => self.limpar = None,
                None => {}
            }
        }
    }
}

/// A conexão aponta para outro lugar (o cache da árvore não vale mais).
fn mudou_o_destino(velha: &api::ConexaoBanco, nova: &api::ConexaoBanco) -> bool {
    velha.banco != nova.banco || velha.host != nova.host || velha.porta != nova.porta || velha.usuario != nova.usuario || velha.arquivo != nova.arquivo
}

/// "Gerar SELECT": o nome qualificado só quando precisa.
fn sql_gerado(tipo: &str, esquema: &str, objeto: &str, limite: u32) -> String {
    let simples =
        |n: &str| !n.is_empty() && !n.starts_with(|c: char| c.is_ascii_digit()) && n.chars().all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit());
    let citar = |n: &str| match tipo {
        "mysql" => format!("`{}`", n.replace('`', "``")),
        "sqlserver" => format!("[{}]", n.replace(']', "]]")),
        _ => format!("\"{}\"", n.replace('"', "\"\"")),
    };
    let nome = |n: &str| if simples(n) { n.to_string() } else { citar(n) };
    let mut alvo = nome(objeto);
    if !esquema.is_empty() && tipo != "mysql" && tipo != "sqlite" {
        alvo = format!("{}.{alvo}", nome(esquema));
    }
    if tipo == "sqlserver" { format!("SELECT TOP {limite} * FROM {alvo};") } else { format!("SELECT * FROM {alvo} LIMIT {limite};") }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn select_gerado_por_dialeto() {
        assert_eq!(sql_gerado("postgres", "public", "pedidos", 100), "SELECT * FROM public.pedidos LIMIT 100;");
        assert_eq!(sql_gerado("sqlserver", "dbo", "Pedidos", 100), "SELECT TOP 100 * FROM dbo.[Pedidos];");
        assert_eq!(sql_gerado("mysql", "", "itens pedido", 100), "SELECT * FROM `itens pedido` LIMIT 100;");
        assert_eq!(sql_gerado("sqlite", "", "clientes", 50), "SELECT * FROM clientes LIMIT 50;");
    }
}
