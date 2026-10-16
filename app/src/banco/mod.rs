//! A tela de bancos de dados do perfil. O painel (na barra lateral, ou à
//! esquerda do cartão com ela recolhida) tem os pedidos dos agentes
//! esperando você e as conexões em árvore; o cartão tem o console da
//! conexão escolhida (editor em cima, resultado ou histórico embaixo) e o
//! pedido aberto numa gaveta. Tudo o que fala com o núcleo roda numa thread
//! e acorda a tela quando volta: nada de consulta periódica; só a execução
//! em andamento redesenha a cada segundo, para o contador.

pub mod aprovacao;
mod arvore;
mod conexao;
mod console;
mod pedido;
mod sql;
mod sugestao;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, CornerRadius, Key, Modifiers, Rect, RichText, Sense, Stroke, pos2, vec2};

use crate::api;
use crate::registro;
use crate::tema::{self, TipoAviso, cores};

/// O que a tela de bancos pede à tela principal.
pub enum Acao {
    AbrirTarefa {
        tarefa: i64,
        agente: i64,
    },
    Avisar(TipoAviso, String),
    /// Aprovar ou recusar um pedido de consulta (a mesma resposta do painel da tarefa).
    Responder {
        id: String,
        resposta: aprovacao::Resposta,
    },
    /// Guardar no perfil o autocompletar do console.
    Autocompletar(bool),
}

/// O que a tela de bancos recebe da principal a cada quadro.
pub struct Contexto<'a> {
    /// Os pedidos dos agentes esperando você.
    pub aprovacoes: &'a [api::Aprovacao],
    /// O que você está fazendo em cada pedido (o mesmo estado do painel da tarefa).
    pub estados: &'a mut HashMap<String, aprovacao::Estado>,
    pub agentes: &'a dyn Fn(i64) -> String,
    /// A branch de uma tarefa.
    pub branch: &'a dyn Fn(i64) -> String,
    /// O painel da barra lateral está à vista com as conexões: a árvore não
    /// se repete no cartão.
    pub arvore_na_lateral: bool,
}

/// "Rodar para mim antes": a execução do SQL de um pedido no seu console.
struct Rodada {
    pedido: api::Aprovacao,
    ficha: String,
    hora: String,
    erro: Option<String>,
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
    RodarPedido(api::Aprovacao),
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
    /// Sugestões no console (do perfil; desligado não calcula nada).
    pub autocompletar: bool,
    /// O pedido aberto na gaveta.
    pedido_aberto: Option<String>,
    rodada: Option<Rodada>,
    /// O histórico já pedido para a gaveta (não pede de novo a cada quadro).
    historico_pedido: Option<i64>,
    /// As colunas pedidas para a sugestão: ao chegarem, sugere de novo (conexão, cursor).
    recalcular: Option<(i64, usize)>,
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
            autocompletar: false,
            pedido_aberto: None,
            rodada: None,
            historico_pedido: None,
            recalcular: None,
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

    fn executar(&mut self, ctx: &egui::Context, conexao: i64, sql: String, banco: String, confirmar: String, agora: f64) -> String {
        let limite = self.console(conexao).limite;
        self.executar_com(ctx, conexao, sql, banco, confirmar, limite, agora)
    }

    #[allow(clippy::too_many_arguments)]
    fn executar_com(&mut self, ctx: &egui::Context, conexao: i64, sql: String, banco: String, confirmar: String, limite: u32, agora: f64) -> String {
        let c = self.console(conexao);
        let ficha = c.nova_ficha(conexao);
        let tempo_s = c.tempo_s;
        c.executando = Some(console::Rodando { ficha: ficha.clone(), desde: agora, mais: false });
        let devolver = ficha.clone();
        self.em_segundo_plano(ctx, move || {
            let resultado = api::executar(conexao, &api::Execucao { ficha: &ficha, sql: &sql, banco: &banco, limite, tempo_s, confirmar: &confirmar });
            Resposta::Executou { conexao, ficha, banco, resultado }
        });
        devolver
    }

    /// "Rodar para mim antes": o SQL do pedido no seu console, com o banco e
    /// o limite do pedido. Só leitura: se o núcleo pedir confirmação de
    /// escrita, a Colmeia não roda (o agente nunca altera dados).
    fn rodar_pedido(&mut self, ctx: &egui::Context, a: &api::Aprovacao, agora: f64) {
        if self.console(a.conexao_id).executando.is_some() {
            self.acoes.push(Acao::Avisar(TipoAviso::Neutro, format!("Já há uma consulta rodando em {}", a.conexao)));
            return;
        }
        let limite = if a.limite > 0 { a.limite as u32 } else { 200 };
        let ficha = self.executar_com(ctx, a.conexao_id, a.sql.clone(), a.banco.clone(), String::new(), limite, agora);
        self.rodada = Some(Rodada { pedido: a.clone(), ficha, hora: String::new(), erro: None });
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
                    let colunas = matches!(pedido, arvore::Pedido::Colunas(..));
                    let mut acoes = Vec::new();
                    self.arvore.receber(pedido, resultado, &tipo, &mut acoes);
                    self.tratar_arvore(ctx, acoes, agora);
                    // Chegaram as colunas que a sugestão pediu: sugere de novo.
                    // Só se o cursor não saiu de onde a sugestão foi pedida.
                    if colunas
                        && let Some((c, cursor)) = self.recalcular
                        && c == con
                        && self.consoles.get(&c).is_some_and(|x| x.cursor_em_bytes(ctx) == Some(cursor))
                    {
                        self.sugerir(ctx, c, cursor);
                    }
                }
                Resposta::Executou { conexao, ficha, banco, resultado } => {
                    let nome = self.conexao(conexao).map(|c| c.nome.clone()).unwrap_or_default();
                    let tipo = self.conexao(conexao).map(|c| c.tipo.clone()).unwrap_or_default();
                    // A execução de "Rodar para mim antes" tem desfecho próprio.
                    if let Some(rodada) = self.rodada.as_mut().filter(|r| r.ficha == ficha) {
                        match &resultado {
                            Ok(_) => {
                                rodada.hora = crate::sistema::hora_local();
                                rodada.erro = None;
                            }
                            Err(api::ErroBanco::Confirmar(_)) => {
                                rodada.erro = Some("Essa instrução altera dados: a Colmeia não roda por você o que o agente pediu.".into())
                            }
                            Err(api::ErroBanco::Senha) => {}
                            Err(e) => rodada.erro = Some(e.texto()),
                        }
                        let a = rodada.pedido.clone();
                        let c = self.console(conexao);
                        if c.executando.as_ref().is_some_and(|e| e.ficha == ficha) {
                            c.executando = None;
                        }
                        match resultado {
                            Ok(r) => {
                                c.historico = None;
                                c.previa_falhou = None;
                                c.resultado_novo(r, ficha, banco, None);
                            }
                            Err(api::ErroBanco::Senha) => self.pedir_senha(ctx, conexao, Pendente::RodarPedido(a)),
                            _ => {}
                        }
                        continue;
                    }
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
                    if self.historico_pedido == Some(conexao) {
                        self.historico_pedido = None;
                    }
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
                            Some(Pendente::RodarPedido(a)) => self.rodar_pedido(ctx, &a, agora),
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

    /// A tela inteira: o console da conexão escolhida no cartão e, com a
    /// barra lateral recolhida, o painel à esquerda dele.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, mut cx: Contexto, agora: f64) -> Vec<Acao> {
        let ctx = ui.ctx().clone();
        self.receber(&ctx, agora);
        if !self.carregou || std::mem::take(&mut self.reler_ao_mostrar) {
            self.recarregar(&ctx);
        }
        // O pedido aberto que foi respondido, expirou ou foi cancelado sai da tela.
        if self.pedido_aberto.as_ref().is_some_and(|id| !cx.aprovacoes.iter().any(|a| a.id == *id)) {
            self.pedido_aberto = None;
        }
        let p = cores();
        let cartao = ui.available_rect_before_wrap();
        if self.carregou && self.conexoes.is_empty() {
            self.vazio(ui);
        } else if let Some(e) = &self.erro_lista {
            ui.label(RichText::new(format!("Não consegui ler as conexões: {e}")).color(p.erro));
        } else {
            self.corpo(ui, &cx, agora);
        }
        self.gaveta(&ctx, cartao, &mut cx, agora);
        self.dialogos(&ctx, agora);
        if self.consoles.values().any(|c| c.executando.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        std::mem::take(&mut self.acoes)
    }

    /// O painel: os pedidos esperando você no topo e as conexões em árvore,
    /// com o filtro. Vai na barra lateral (ou à esquerda do cartão).
    pub fn painel(&mut self, ui: &mut egui::Ui, aprovacoes: &[api::Aprovacao]) {
        let ctx = ui.ctx().clone();
        let p = cores();
        if !aprovacoes.is_empty() {
            rotulo(ui, "ESPERANDO VOCÊ", Some(aprovacoes.len()));
            ui.add_space(4.0);
            for a in aprovacoes.iter().take(4) {
                let ativo = self.pedido_aberto.as_deref() == Some(a.id.as_str());
                if cartao_esperando(ui, a, ativo).clicked() {
                    self.abrir_pedido(&ctx, a);
                }
                ui.add_space(5.0);
            }
            if aprovacoes.len() > 4 {
                ui.label(RichText::new(format!("e mais {} na barra das tarefas", aprovacoes.len() - 4)).color(p.suave).size(12.0));
            }
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(r.x_range().shrink(6.0), r.center().y, Stroke::new(1.0, p.borda));
            ui.add_space(8.0);
        }
        let largura = ui.available_width();
        let (linha, _) = ui.allocate_exact_size(vec2(largura, 24.0), Sense::hover());
        ui.painter().text(linha.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, "CONEXÕES", tema::forte(10.5), p.suave);
        let mais = Rect::from_center_size(pos2(linha.right() - 18.0, linha.center().y), vec2(22.0, 22.0));
        if tema::botao_icone_em(ui, mais, egui::Id::new("nova-conexao-painel"), tema::Icone::MaisZoom).on_hover_text("Nova conexão").clicked() {
            self.nova_conexao("postgres");
        }
        ui.add_space(4.0);
        self.arvore.pedidos = aprovacoes.iter().map(|a| a.conexao_id).collect();
        let acoes = self.arvore.mostrar(ui, &self.conexoes);
        self.tratar_arvore(&ctx, acoes, 0.0);
    }

    fn abrir_pedido(&mut self, ctx: &egui::Context, a: &api::Aprovacao) {
        self.pedido_aberto = Some(a.id.clone());
        self.escolhida = Some(a.conexao_id);
        self.arvore.escolhido = Some(arvore::No::Conexao(a.conexao_id));
        self.console(a.conexao_id).sugestoes = None;
        // O teclado sai do editor: o que se digita vai para a gaveta.
        ctx.memory_mut(|m| m.surrender_focus(console::id_editor()));
    }

    /// A gaveta do pedido aberto, por cima do cartão.
    fn gaveta(&mut self, ctx: &egui::Context, cartao: Rect, cx: &mut Contexto, agora: f64) {
        let Some(id) = self.pedido_aberto.clone() else { return };
        let Some(a) = cx.aprovacoes.iter().find(|a| a.id == id) else { return };
        let con = a.conexao_id;
        // "Antes, nesta conexão" sai do histórico da conexão (pedido uma vez).
        if self.console(con).historico.is_none() && self.historico_pedido != Some(con) {
            self.historico_pedido = Some(con);
            self.ler_historico(ctx, con);
        }
        let c = &self.consoles[&con];
        let rodada = self.rodada.as_ref().filter(|r| r.pedido.id == id);
        let rodando = rodada.is_some_and(|r| c.executando.as_ref().is_some_and(|e| e.ficha == r.ficha));
        let vista_rodada = rodada.and_then(|r| match (&r.erro, &c.painel) {
            (Some(e), _) => Some(pedido::Rodada::Erro(e.as_str())),
            (None, console::Painel::Resultado { r: res, ficha, .. }) if *ficha == r.ficha => Some(pedido::Rodada::Resultado { r: res, hora: &r.hora }),
            _ => None,
        });
        let antes = match &c.historico {
            Some(Ok(h)) => Some(h.iter().filter(|x| x.origem == "agente" && x.agente_id == a.agente_id).take(5).collect()),
            Some(Err(_)) => Some(Vec::new()),
            None => None,
        };
        let branch = (cx.branch)(a.tarefa_id);
        let vista = pedido::Vista { a, branch: &branch, chaveiro: self.chaveiro, rodada: vista_rodada, rodando, antes };
        let estado = cx.estados.entry(id.clone()).or_insert_with(aprovacao::Estado::novo);
        let resposta = pedido::gaveta(ctx, cartao, &vista, estado);
        match resposta {
            None => {}
            Some(pedido::Pedido::Fechar) => self.pedido_aberto = None,
            Some(pedido::Pedido::AbrirTarefa) => self.acoes.push(Acao::AbrirTarefa { tarefa: a.tarefa_id, agente: a.agente_id }),
            Some(pedido::Pedido::Rodar) => {
                let a = a.clone();
                self.rodar_pedido(ctx, &a, agora);
            }
            Some(pedido::Pedido::Responder(resposta)) => self.acoes.push(Acao::Responder { id, resposta }),
        }
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

    fn corpo(&mut self, ui: &mut egui::Ui, cx: &Contexto, agora: f64) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let area = ui.available_rect_before_wrap();
        let r_console = if cx.arvore_na_lateral {
            area
        } else {
            // A barra lateral recolhida: o painel fica à esquerda do console.
            let maxima = (area.width() * 0.45).max(220.0);
            self.largura_arvore = self.largura_arvore.clamp(220.0, maxima);
            let r_arvore = Rect::from_min_size(area.min, vec2(self.largura_arvore, area.height()));
            let r_divisa = Rect::from_min_size(pos2(r_arvore.right(), area.top()), vec2(8.0, area.height()));
            ui.painter().rect_filled(r_arvore, CornerRadius::same(tema::RAIO_SUPERFICIE), p.superficie);
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(r_arvore.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            self.painel(&mut filho, cx.aprovacoes);
            self.largura_arvore += tema::divisoria(ui, r_divisa, egui::Id::new("divisa-arvore"), true);
            Rect::from_min_max(pos2(r_divisa.right() + 6.0, area.top()), area.max)
        };
        // Ctrl+F: o filtro da árvore (fora do editor).
        let editor_com_foco = ctx.memory(|m| m.has_focus(console::id_editor()));
        if !editor_com_foco && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
            self.arvore.focar_filtro = true;
        }
        // O console da conexão escolhida.
        let Some(id) = self.escolhida.filter(|e| self.conexoes.iter().any(|c| c.id == *e)) else {
            ui.painter().text(r_console.center(), egui::Align2::CENTER_CENTER, "Escolha uma conexão no painel.", egui::FontId::proportional(13.0), p.suave);
            return;
        };
        let conexao = self.conexao(id).cloned().unwrap_or_default();
        let bancos: Vec<String> = match self.arvore.bancos.get(&id) {
            Some(arvore::Carga::Pronta(n)) => n.nomes.clone(),
            _ => Vec::new(),
        };
        let mut acoes = Vec::new();
        let cab = Rect::from_min_size(r_console.min, vec2(r_console.width(), 40.0));
        self.cabecalho_conexao(ui, cab, &conexao, &ctx);
        let mut y = cab.bottom() + 12.0;
        // O pedido de um agente nesta conexão: a faixa abre a gaveta.
        let desta: Vec<&api::Aprovacao> = cx.aprovacoes.iter().filter(|a| a.conexao_id == id).collect();
        if let Some(a) = desta.first() {
            let faixa = Rect::from_min_size(pos2(r_console.left(), y), vec2(r_console.width(), 40.0));
            if faixa_do_pedido(ui, faixa, a, desta.len()) {
                let a = (*a).clone();
                self.abrir_pedido(&ctx, &a);
            }
            y = faixa.bottom() + 10.0;
        }
        let barra = Rect::from_min_size(pos2(r_console.left(), y), vec2(r_console.width(), 34.0));
        let resto = Rect::from_min_max(pos2(r_console.left(), barra.bottom() + 10.0), r_console.max);
        let altura_editor = ((resto.height() - 8.0) * self.fracao_editor).clamp(120.0, (resto.height() - 8.0 - 160.0).max(120.0));
        let r_editor = Rect::from_min_size(resto.min, vec2(resto.width(), altura_editor));
        let r_divisa2 = Rect::from_min_size(pos2(resto.left(), r_editor.bottom()), vec2(resto.width(), 8.0));
        let r_painel = Rect::from_min_max(pos2(resto.left(), r_divisa2.bottom()), resto.max);
        let autocompletar = self.autocompletar;
        {
            let c = self.console(id);
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(barra));
            c.barra(&mut filho, &conexao, &bancos, autocompletar, agora, &mut acoes);
            c.editor(ui, r_editor, &conexao.tipo, autocompletar, agora, &mut acoes);
        }
        let d = tema::divisoria(ui, r_divisa2, egui::Id::new("divisa-editor"), false);
        if d != 0.0 && resto.height() > 0.0 {
            self.fracao_editor = ((altura_editor + d) / (resto.height() - 8.0)).clamp(0.15, 0.85);
        }
        let padrao = conexao.banco.clone();
        self.console(id).painel(ui, r_painel, &conexao.tipo, &padrao, cx.agentes, &mut acoes);
        for a in acoes {
            match a {
                console::Acao::Executar => {
                    let c = self.console(id);
                    if c.executando.is_some() {
                        continue;
                    }
                    c.sugestoes = None;
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
                console::Acao::Sugerir(cursor) => self.sugerir(&ctx, id, cursor),
                console::Acao::Autocompletar(ligado) => {
                    self.autocompletar = ligado;
                    self.acoes.push(Acao::Autocompletar(ligado));
                }
            }
        }
        if self.autocompletar {
            self.console(id).popup(&ctx);
        }
    }

    /// O nome da conexão em destaque, o tipo e onde ela aponta; à direita,
    /// Testar e Editar.
    fn cabecalho_conexao(&mut self, ui: &mut egui::Ui, rect: Rect, c: &api::ConexaoBanco, ctx: &egui::Context) {
        let p = cores();
        let selo = Rect::from_min_size(pos2(rect.left(), rect.center().y - 17.0), vec2(34.0, 34.0));
        ui.painter().rect_filled(selo, CornerRadius::same(10), tema::fundo_tingido(p, p.destaque, tema::claro()));
        tema::desenhar_icone(ui.painter(), selo.center(), tema::Icone::Banco, p.destaque);
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)));
        filho.spacing_mut().item_spacing.x = 8.0;
        if tema::botao_secundario(&mut filho, "Editar conexão").clicked() {
            self.dialogo = Some(conexao::Dialogo::editar(c));
        }
        if tema::botao_secundario(&mut filho, "Testar").on_hover_text("Conecta e diz quanto demorou e se usou TLS").clicked() {
            self.testar(ctx, c.id);
        }
        let direita = filho.min_rect().left() - 12.0;
        let x = selo.right() + 12.0;
        let nome = tema::cortar(ui.painter(), &c.nome, egui::TextFormat::simple(tema::forte(19.0), p.texto), (direita - x - 80.0).max(60.0), 1, true);
        let y = rect.center().y - (nome.size().y + 16.0) / 2.0;
        ui.painter().galley(pos2(x, y), nome.clone(), p.texto);
        let sigla = tema::etiqueta(
            ui.painter(),
            pos2(x + nome.size().x + 8.0, y + nome.size().y / 2.0 - 9.5),
            console::sigla_do_tipo(&c.tipo),
            tema::fonte_etiqueta(),
            p.suave,
        );
        ui.interact(sigla, egui::Id::new(("sigla-conexao", c.id)), Sense::hover()).on_hover_text(console::nome_do_tipo(&c.tipo));
        let mut partes = Vec::new();
        if let Some(e) = self.arvore.erros.get(&c.id) {
            partes.push(e.clone());
        } else if c.tipo == "sqlite" {
            partes.push(c.arquivo.clone());
        } else {
            partes.push(format!("{}:{}", c.host, c.porta));
            if !c.banco.is_empty() {
                partes.push(format!("banco {}", c.banco));
            }
        }
        if c.agentes {
            partes.push("agentes podem pedir consultas".into());
        }
        let erro = self.arvore.erros.contains_key(&c.id);
        let mut x_sub = x;
        if erro {
            ui.painter().circle_filled(pos2(x + 3.0, y + nome.size().y + 9.0), 3.0, p.erro);
            x_sub += 12.0;
        }
        let sub = tema::cortar(
            ui.painter(),
            &partes.join(" · "),
            egui::TextFormat::simple(egui::FontId::proportional(12.0), p.suave),
            (direita - x_sub).max(60.0),
            1,
            true,
        );
        ui.painter().galley(pos2(x_sub, y + nome.size().y + 2.0), sub, p.suave);
    }

    /// Sugere para o cursor (em bytes) com o que a árvore já carregou; pede
    /// as colunas que faltam pela rota da árvore (e sugere de novo ao chegarem).
    fn sugerir(&mut self, ctx: &egui::Context, id: i64, cursor: usize) {
        self.recalcular = None;
        let Some(conexao) = self.conexao(id).cloned() else { return };
        let Some(c) = self.consoles.get(&id) else { return };
        let banco = if c.banco.is_empty() { conexao.banco.clone() } else { c.banco.clone() };
        let catalogo = self.catalogo(id, &conexao.tipo, &banco);
        let Some(analise) = sugestao::analisar(&conexao.tipo, &c.texto, cursor) else {
            self.console(id).sugestoes = None;
            return;
        };
        let mut pediu = false;
        for (esquema, tabela) in sugestao::colunas_que_faltam(&analise, &catalogo) {
            let achada = self.arvore.objetos.iter().find_map(|((c, b, e), carga)| match carga {
                arvore::Carga::Pronta(o) if *c == id && *e == esquema && o.tabelas.iter().chain(&o.views).any(|t| *t == tabela) => Some(b.clone()),
                _ => None,
            });
            if let Some(b) = achada {
                let chave = (id, b.clone(), esquema.clone(), tabela.clone());
                if let std::collections::hash_map::Entry::Vacant(v) = self.arvore.colunas.entry(chave) {
                    v.insert(arvore::Carga::Carregando);
                    self.pedir_arvore(ctx, arvore::Pedido::Colunas(id, b, esquema, tabela));
                    pediu = true;
                }
            }
        }
        let itens = sugestao::sugerir(&conexao.tipo, &analise, &catalogo);
        let c = self.console(id);
        let ancora = c.ancora_cursor.unwrap_or_default();
        c.sugestoes = (!itens.is_empty()).then_some(console::Popup { itens, escolhido: 0, inicio: analise.inicio, cursor, ancora, rect: Rect::NOTHING });
        if pediu {
            self.recalcular = Some((id, cursor));
        }
        ctx.request_repaint();
    }

    /// Os nomes que a árvore já carregou da conexão, no banco do console.
    fn catalogo(&self, id: i64, tipo: &str, banco: &str) -> sugestao::Catalogo {
        let mut cat = sugestao::Catalogo::default();
        let do_banco = |b: &str| tipo == "sqlite" || banco.is_empty() || b == banco;
        for ((c, b), carga) in &self.arvore.esquemas {
            if *c == id
                && do_banco(b)
                && let arvore::Carga::Pronta(nomes) = carga
            {
                cat.esquemas.extend(nomes.iter().cloned());
            }
        }
        for ((c, b, e), carga) in &self.arvore.objetos {
            if *c == id
                && do_banco(b)
                && let arvore::Carga::Pronta(o) = carga
            {
                cat.tabelas.extend(o.tabelas.iter().chain(&o.views).map(|t| (e.clone(), t.clone())));
            }
        }
        for ((c, b, _, t), carga) in &self.arvore.colunas {
            if *c == id
                && do_banco(b)
                && let arvore::Carga::Pronta(cs) = carga
            {
                cat.colunas.insert(t.to_lowercase(), cs.iter().map(|x| (x.nome.clone(), arvore::tipo_curto(&x.tipo))).collect());
            }
        }
        cat.esquemas.sort();
        cat.esquemas.dedup();
        cat
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

/// O rótulo de uma seção do painel ("ESPERANDO VOCÊ"), com a contagem.
fn rotulo(ui: &mut egui::Ui, texto: &str, n: Option<usize>) {
    let p = cores();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(r.left_center() + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, texto, tema::forte(10.5), p.suave);
    if let Some(n) = n {
        let g = ui.painter().layout_no_wrap(n.to_string(), tema::forte(11.0), tema::sobre_destaque());
        let selo =
            Rect::from_center_size(pos2(r.right() - 10.0 - (g.size().x + 10.0).max(18.0) / 2.0, r.center().y), vec2((g.size().x + 10.0).max(18.0), 18.0));
        ui.painter().rect_filled(selo, CornerRadius::same(9), p.alerta);
        ui.painter().galley(selo.center() - g.size() / 2.0, g, tema::sobre_destaque());
    }
}

/// Um pedido esperando você no painel: quem, o que e em que tarefa.
fn cartao_esperando(ui: &mut egui::Ui, a: &api::Aprovacao, ativo: bool) -> egui::Response {
    let p = cores();
    let largura = ui.available_width() - 4.0;
    let (rect, resposta) = ui.allocate_exact_size(vec2(largura, 64.0), Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    let forte_ = tema::transicao(ui, resposta.id.with("mouse"), ativo || resposta.hovered());
    let base = tema::fundo_tingido(p, p.alerta, tema::claro());
    let fundo = tema::misturar(p.superficie, base, 0.55 + 0.45 * forte_);
    ui.painter().rect(rect, CornerRadius::same(12), fundo, Stroke::new(1.0, p.alerta.gamma_multiply(0.3 + 0.4 * forte_)), egui::StrokeKind::Inside);
    ui.painter().rect_filled(Rect::from_min_size(rect.min + vec2(6.0, 10.0), vec2(3.0, rect.height() - 20.0)), CornerRadius::same(2), p.alerta);
    let x = rect.left() + 16.0;
    let largura_texto = rect.right() - 10.0 - x;
    let quem = if a.agente.is_empty() { "Um agente" } else { a.agente.as_str() };
    let prazo = if a.expira_hora.is_empty() { String::new() } else { format!("até {}", a.expira_hora) };
    let g_prazo = ui.painter().layout_no_wrap(prazo, tema::forte(11.5), p.alerta);
    ui.painter().galley(pos2(rect.right() - 10.0 - g_prazo.size().x, rect.top() + 9.0), g_prazo.clone(), p.alerta);
    let g = tema::cortar(ui.painter(), quem, egui::TextFormat::simple(tema::forte(13.0), p.texto), largura_texto - g_prazo.size().x - 8.0, 1, true);
    ui.painter().galley(pos2(x, rect.top() + 8.0), g, p.texto);
    let senha = if a.precisa_senha { " · pede a senha" } else { "" };
    let o_que = format!("quer consultar {}{senha}", a.conexao);
    let g = tema::cortar(ui.painter(), &o_que, egui::TextFormat::simple(egui::FontId::proportional(12.5), p.suave), largura_texto, 1, true);
    ui.painter().galley(pos2(x, rect.top() + 27.0), g, p.suave);
    let g = tema::cortar(ui.painter(), &a.tarefa, egui::TextFormat::simple(egui::FontId::proportional(11.5), p.suave), largura_texto, 1, true);
    ui.painter().galley(pos2(x, rect.top() + 44.0), g, p.suave);
    resposta.on_hover_text("Abrir o pedido")
}

/// A faixa do pedido no topo do console: clicar abre a gaveta.
fn faixa_do_pedido(ui: &mut egui::Ui, rect: Rect, a: &api::Aprovacao, n: usize) -> bool {
    let p = cores();
    let resposta = ui.interact(rect, egui::Id::new(("faixa-pedido", &a.id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = tema::transicao(ui, resposta.id.with("mouse"), resposta.hovered());
    let fundo = tema::misturar(tema::fundo_tingido(p, p.alerta, tema::claro()), p.alerta, 0.05 + 0.05 * t);
    ui.painter().rect(rect, CornerRadius::same(14), fundo, Stroke::new(1.0, p.alerta.gamma_multiply(0.45)), egui::StrokeKind::Inside);
    ui.painter().circle_filled(pos2(rect.left() + 18.0, rect.center().y), 4.0, p.alerta);
    let botao = Rect::from_min_size(pos2(rect.right() - 6.0 - 104.0, rect.top() + 6.0), vec2(104.0, rect.height() - 12.0));
    let clicou_botao = ui.put(botao, |ui: &mut egui::Ui| tema::botao_secundario(ui, "Ver pedido")).clicked();
    let prazo = if a.expira_hora.is_empty() { String::new() } else { format!("expira às {}", a.expira_hora) };
    let g_prazo = ui.painter().layout_no_wrap(prazo, tema::forte(12.5), p.alerta);
    let x_prazo = botao.left() - 12.0 - g_prazo.size().x;
    ui.painter().galley(pos2(x_prazo, rect.center().y - g_prazo.size().y / 2.0), g_prazo, p.alerta);
    let quem = if a.agente.is_empty() { "Um agente" } else { a.agente.as_str() };
    let mais = if n > 1 { format!(" (e mais {})", n - 1) } else { String::new() };
    let mut job = egui::text::LayoutJob::default();
    job.append(quem, 0.0, egui::TextFormat::simple(tema::forte(13.0), p.texto));
    job.append(&format!(" quer consultar esta conexão{mais}"), 0.0, egui::TextFormat::simple(egui::FontId::proportional(13.0), p.texto));
    if !a.tarefa.is_empty() {
        job.append(&format!(" · {}", a.tarefa), 0.0, egui::TextFormat::simple(egui::FontId::proportional(13.0), p.suave));
    }
    job.wrap =
        egui::text::TextWrapping { max_width: (x_prazo - rect.left() - 44.0).max(40.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(rect.left() + 32.0, rect.center().y - g.size().y / 2.0), g, p.texto);
    resposta.clicked() || clicou_botao
}

/// "Gerar SELECT": o nome qualificado só quando precisa.
fn sql_gerado(tipo: &str, esquema: &str, objeto: &str, limite: u32) -> String {
    let mut alvo = sugestao::nome_sql(tipo, objeto);
    if !esquema.is_empty() && tipo != "mysql" && tipo != "sqlite" {
        alvo = format!("{}.{alvo}", sugestao::nome_sql(tipo, esquema));
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
