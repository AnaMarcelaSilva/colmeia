//! Tela da Colmeia: entrada por perfil, quadro de tarefas por projeto, painel
//! da tarefa (só o terminal em foco é tempo real, com caixa de mensagem para os
//! agentes) e a abelha da barra lateral, que resume o que mais precisa de você.
//! Fala com o núcleo em Go pelo canal local (socket Unix + token).

mod abelha;
mod api;
mod canal;
mod compositor;
mod dados;
mod dialogos;
mod entrada;
mod quadro;
mod sistema;
mod tema;
mod terminal;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use abelha::Abelha;
use compositor::Compositor;
use dados::{AgenteTela, Coluna, Projeto, Tarefa};
use dialogos::{Dialogo, Resultado};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};
use mascote::Estado;
use tema::{cores, texto_forte};
use terminal::{MINIATURA, SO_CARTAO, TEMPO_REAL, TerminalAgente, pedir_carga};

/// O que está na tela: todos os projetos do perfil ou um projeto. A abelha resume o escopo.
#[derive(Clone, Copy, PartialEq)]
enum Escopo {
    Perfil,
    Projeto(i64),
}

impl Escopo {
    fn contem(self, projeto: i64) -> bool {
        match self {
            Escopo::Perfil => true,
            Escopo::Projeto(p) => p == projeto,
        }
    }
}

enum Tela {
    Entrada(Box<entrada::Entrada>),
    Quadro,
    /// `foco` é o agente em tempo real (0 quando a tarefa não tem agentes).
    Tarefa {
        id: i64,
        foco: i64,
    },
}

struct Colmeia {
    /// O núcleo está em modo demonstração (dados de exemplo, cargas de teste e cenários simulados).
    demo: bool,
    /// Problema com o núcleo, mostrado no topo da tela.
    aviso: Option<String>,
    /// Aviso passageiro no rodapé (ex.: um erro ao mover uma tarefa).
    recado: Option<(String, f64)>,
    perfil: Option<api::Perfil>,
    perfis: Vec<api::Perfil>,
    projetos: Vec<Projeto>,
    tarefas: Vec<Tarefa>,
    branches: Vec<String>,
    /// Terminais dos agentes, pelo id do agente (na demonstração, o número do terminal de teste).
    terminais: HashMap<i64, TerminalAgente>,
    /// Ferramentas instaladas e contas do perfil, lidas quando precisa.
    ferramentas: Option<Vec<api::Ferramenta>>,
    contas: Option<Vec<api::Conta>>,
    /// Editor para abrir a pasta da tarefa (IntelliJ ou VS Code), se houver.
    editor: Option<(&'static str, &'static str)>,
    tela: Tela,
    escopo: Escopo,
    dialogo: Option<Dialogo>,
    abelha: Abelha,
    sem_abelha: bool,
    tema: tema::Escolha,
    favo: tema::Favo,
    compositor: Compositor,
    filtro: Option<String>,
    carga: &'static str,
    bytes: Arc<AtomicU64>,
    quadros: u64,
    // Medidor: valores do último segundo.
    ultimo_segundo: f64,
    bytes_antes: u64,
    quadros_antes: u64,
    fps: u64,
    vazao: u64,
}

impl Colmeia {
    fn new(cc: &eframe::CreationContext, aviso: Option<String>) -> Self {
        tema::instalar(&cc.egui_ctx);
        // COLMEIA_TEMA=claro | escuro | leitura escolhe o tema antes de entrar num perfil.
        let escolha = tema::Escolha::da_chave(&std::env::var("COLMEIA_TEMA").unwrap_or_default());
        escolha.aplicar(&cc.egui_ctx);
        let demo = canal::pedir("GET", "/v1/versao").is_ok_and(|v| v.contains("\"demo\":true"));
        let bytes = Arc::new(AtomicU64::new(0));

        let mut app = Colmeia {
            demo,
            aviso,
            recado: None,
            perfil: None,
            perfis: Vec::new(),
            projetos: Vec::new(),
            tarefas: Vec::new(),
            branches: Vec::new(),
            terminais: HashMap::new(),
            ferramentas: None,
            contas: None,
            editor: sistema::editor(),
            tela: Tela::Quadro,
            escopo: Escopo::Perfil,
            dialogo: None,
            abelha: Abelha::new(),
            sem_abelha: std::env::var("COLMEIA_SEM_ABELHA").is_ok_and(|v| v == "1"),
            tema: escolha,
            favo: tema::Favo::default(),
            compositor: Compositor::default(),
            filtro: None,
            carga: "parada",
            bytes: bytes.clone(),
            quadros: 0,
            ultimo_segundo: 0.0,
            bytes_antes: 0,
            quadros_antes: 0,
            fps: 0,
            vazao: 0,
        };

        app.compositor.focar = true;
        if !demo {
            app.tela = Tela::Entrada(Box::new(entrada::Entrada::new(false)));
            return app;
        }

        // Modo demonstração: dados de exemplo e os terminais de teste do núcleo.
        // COLMEIA_CARTOES=500, COLMEIA_TAREFA=101 e COLMEIA_CENARIO=erro ajudam a medir sem clicar.
        app.perfil = Some(api::Perfil { id: 0, nome: "Demonstração".into(), tema: escolha.chave().into() });
        app.projetos = dados::projetos_demo();
        let cartoes = std::env::var("COLMEIA_CARTOES").ok().and_then(|v| v.parse().ok()).unwrap_or(50);
        app.tarefas = dados::gerar_demo(cartoes);
        if std::env::var("COLMEIA_CENARIO").is_ok_and(|v| v == "erro")
            && let Some(t) = app.tarefas.iter_mut().find(|t| t.id == 103)
        {
            t.erro = Some("agente-6 parou: 3 testes falhando");
        }
        app.terminais = (0..dados::PAPEIS_DEMO.len() as i64)
            .map(|id| (id, TerminalAgente::conectar(format!("/v1/terminais/{id}"), cc.egui_ctx.clone(), bytes.clone(), SO_CARTAO)))
            .collect();
        app.escopo = Escopo::Projeto(1);
        app.carregar_branches();
        if let Some(t) = std::env::var("COLMEIA_TAREFA").ok().and_then(|v| v.parse::<i64>().ok()).and_then(|id| app.tarefas.iter().find(|t| t.id == id)) {
            app.tela = Tela::Tarefa { id: t.id, foco: t.agentes.first().map_or(0, |a| a.id) };
        }
        app
    }

    fn medir(&mut self, agora: f64) {
        self.quadros += 1;
        if agora - self.ultimo_segundo >= 1.0 {
            let bytes = self.bytes.load(Ordering::Relaxed);
            self.fps = self.quadros - self.quadros_antes;
            self.vazao = bytes - self.bytes_antes;
            self.quadros_antes = self.quadros;
            self.bytes_antes = bytes;
            self.ultimo_segundo = agora;
        }
    }

    fn avisar(&mut self, texto: impl Into<String>, agora: f64) {
        self.recado = Some((texto.into(), agora));
    }

    // Perfil, projetos e tarefas

    fn entrar(&mut self, perfil: api::Perfil, ctx: &egui::Context) {
        self.tema = tema::Escolha::da_chave(&perfil.tema);
        self.tema.aplicar(ctx);
        self.perfis = api::perfis().unwrap_or_default();
        self.perfil = Some(perfil);
        self.filtro = None;
        self.tela = Tela::Quadro;
        // Os terminais do perfil anterior saem da tela; no núcleo eles continuam rodando.
        self.terminais.clear();
        self.contas = None;
        self.recarregar(ctx.input(|i| i.time));
        self.conectar_agentes(ctx);
        self.escopo = self.projetos.first().map_or(Escopo::Perfil, |p| Escopo::Projeto(p.id));
        self.carregar_branches();
    }

    /// Lê de novo projetos e tarefas do perfil no núcleo.
    fn recarregar(&mut self, agora: f64) {
        if self.demo {
            return;
        }
        let Some(perfil) = &self.perfil else { return };
        match api::projetos(perfil.id) {
            Ok(lista) => self.projetos = lista.into_iter().map(Projeto::from).collect(),
            Err(e) => return self.avisar(format!("Não consegui ler os projetos: {e}"), agora),
        }
        let mut tarefas = Vec::new();
        for projeto in &self.projetos {
            let inicio = tarefas.len();
            match api::tarefas(projeto.id) {
                Ok(lista) => tarefas.extend(lista.into_iter().map(|t| Tarefa::da_api(t, projeto))),
                Err(e) => self.recado = Some((format!("Não consegui ler as tarefas de {}: {e}", projeto.nome), agora)),
            }
            // Os agentes do projeto vêm de uma vez e são distribuídos pelas tarefas.
            for a in api::agentes_do_projeto(projeto.id).unwrap_or_default() {
                if let Some(t) = tarefas[inicio..].iter_mut().find(|t| t.id == a.tarefa_id) {
                    t.agentes.push(AgenteTela { id: a.id, ferramenta: a.ferramenta, papel: a.papel, ativo: a.ativo });
                }
            }
        }
        self.tarefas = tarefas;
        if let Escopo::Projeto(id) = self.escopo
            && !self.projetos.iter().any(|p| p.id == id)
        {
            self.escopo = Escopo::Perfil;
        }
    }

    /// Liga a tela aos terminais dos agentes que estão rodando no núcleo e
    /// solta os que não existem mais.
    fn conectar_agentes(&mut self, ctx: &egui::Context) {
        if self.demo {
            return;
        }
        let ativos: Vec<i64> = self.tarefas.iter().flat_map(|t| t.agentes.iter().filter(|a| a.ativo).map(|a| a.id)).collect();
        self.terminais.retain(|id, _| ativos.contains(id));
        for id in ativos {
            self.terminais
                .entry(id)
                .or_insert_with(|| TerminalAgente::conectar(format!("/v1/agentes/{id}/terminal"), ctx.clone(), self.bytes.clone(), SO_CARTAO));
        }
    }

    /// Ferramentas que o perfil pode abrir como agente: as instaladas que o perfil
    /// escolheu (ou todas as instaladas, se não escolheu nenhuma) e um terminal comum.
    fn opcoes_de_agente(&mut self) -> Vec<dialogos::Opcao> {
        let perfil = self.perfil.as_ref().map(|p| p.id);
        let contas = self.contas.get_or_insert_with(|| perfil.and_then(|p| api::contas(p).ok()).unwrap_or_default());
        let instaladas = self.ferramentas.get_or_insert_with(|| api::ferramentas().unwrap_or_default());
        let mut opcoes: Vec<dialogos::Opcao> = instaladas
            .iter()
            .filter(|f| f.instalada && (contas.is_empty() || contas.iter().any(|c| c.ferramenta == f.id)))
            .map(|f| dialogos::Opcao { id: f.id.clone(), nome: f.nome.clone() })
            .collect();
        opcoes.push(dialogos::Opcao { id: "shell".into(), nome: "Terminal".into() });
        opcoes
    }

    fn adicionar_agente(&mut self, tarefa: i64, retomar: bool) {
        let Some(t) = self.tarefas.iter().find(|t| t.id == tarefa) else {
            return;
        };
        let pasta = t.pasta.clone();
        let opcoes = self.opcoes_de_agente();
        self.dialogo = Some(Dialogo::NovoAgente(dialogos::NovoAgente::new(tarefa, pasta, opcoes, retomar)));
    }

    /// O agente acabou de ser criado: liga o terminal, põe em foco e, se a
    /// tarefa estava no Backlog, passa para "Agente trabalhando".
    fn agente_criado(&mut self, a: api::Agente, ctx: &egui::Context, agora: f64) {
        let terminal = TerminalAgente::conectar(format!("/v1/agentes/{}/terminal", a.id), ctx.clone(), self.bytes.clone(), TEMPO_REAL);
        self.terminais.insert(a.id, terminal);
        let Some(t) = self.tarefas.iter_mut().find(|t| t.id == a.tarefa_id) else {
            return;
        };
        t.agentes.push(AgenteTela { id: a.id, ferramenta: a.ferramenta, papel: a.papel, ativo: true });
        let estava_no_backlog = t.coluna == Coluna::Backlog;
        if estava_no_backlog {
            t.coluna = Coluna::Trabalhando;
        }
        self.tela = Tela::Tarefa { id: a.tarefa_id, foco: a.id };
        if estava_no_backlog {
            self.mover(a.tarefa_id, Coluna::Trabalhando, agora);
        }
    }

    fn iniciar_agente(&mut self, id: i64, ctx: &egui::Context, agora: f64) {
        match api::iniciar_agente(id) {
            Ok(_) => {
                let terminal = TerminalAgente::conectar(format!("/v1/agentes/{id}/terminal"), ctx.clone(), self.bytes.clone(), TEMPO_REAL);
                self.terminais.insert(id, terminal);
            }
            Err(e) => self.avisar(format!("Não consegui iniciar o agente: {e}"), agora),
        }
    }

    fn remover_agente(&mut self, id: i64, agora: f64) {
        if let Err(e) = api::remover_agente(id) {
            return self.avisar(format!("Não consegui remover o agente: {e}"), agora);
        }
        self.terminais.remove(&id);
        for t in &mut self.tarefas {
            t.agentes.retain(|a| a.id != id);
        }
        if let Tela::Tarefa { id: tarefa, foco } = self.tela
            && foco == id
        {
            let proximo = self.tarefas.iter().find(|t| t.id == tarefa).and_then(|t| t.agentes.first()).map_or(0, |a| a.id);
            self.tela = Tela::Tarefa { id: tarefa, foco: proximo };
        }
    }

    fn carregar_branches(&mut self) {
        self.branches = if self.demo {
            dados::BRANCHES_DEMO.iter().map(|b| b.to_string()).collect()
        } else if let Escopo::Projeto(id) = self.escopo {
            api::branches(id).unwrap_or_default()
        } else {
            let mut todas: Vec<String> = self.tarefas.iter().map(|t| t.branch.clone()).collect();
            todas.sort();
            todas.dedup();
            todas
        };
    }

    fn mudar_escopo(&mut self, escopo: Escopo) {
        self.escopo = escopo;
        self.tela = Tela::Quadro;
        self.filtro = None;
        self.carregar_branches();
    }

    fn projeto_em_foco(&self) -> Option<&Projeto> {
        match self.escopo {
            Escopo::Projeto(id) => self.projetos.iter().find(|p| p.id == id),
            Escopo::Perfil => None,
        }
    }

    /// Cada terminal recebe no ritmo que a tela precisa: tempo real só para o
    /// que está em foco, miniatura para os outros da tarefa, e o mínimo para
    /// quem só aparece como última linha num cartão.
    fn ajustar_ritmos(&self) {
        let (foco, tarefa) = match self.tela {
            Tela::Tarefa { id, foco } => (Some(foco), self.tarefas.iter().find(|t| t.id == id)),
            _ => (None, None),
        };
        for (&i, t) in &self.terminais {
            let ms = if Some(i) == foco {
                TEMPO_REAL
            } else if tarefa.is_some_and(|t| t.agentes.iter().any(|a| a.id == i)) {
                MINIATURA
            } else {
                SO_CARTAO
            };
            t.definir_intervalo(ms);
        }
    }

    fn abrir_tarefa(&mut self, id: i64) {
        let Some(t) = self.tarefas.iter().find(|t| t.id == id) else {
            return;
        };
        let (projeto, foco) = (t.projeto_id, t.agentes.first().map_or(0, |a| a.id));
        if !self.escopo.contem(projeto) {
            self.escopo = Escopo::Projeto(projeto);
            self.carregar_branches();
        }
        self.tela = Tela::Tarefa { id, foco };
        self.abelha.resumo_aberto = false;
        self.compositor.focar = true;
    }

    fn mover(&mut self, id: i64, coluna: Coluna, agora: f64) {
        let Some(t) = self.tarefas.iter().find(|t| t.id == id) else {
            return;
        };
        let projeto = t.projeto_id;
        if !self.demo
            && let Err(e) = api::mover_tarefa(id, coluna.chave())
        {
            self.avisar(format!("Não consegui mover a tarefa: {e}"), agora);
            self.recarregar(agora);
            return;
        }
        if coluna == Coluna::Concluido {
            self.abelha.concluiu(id, projeto, agora);
        }
    }

    // Cenários da demonstração para testar a abelha sem esperar um erro de verdade.

    fn concluir_demo(&mut self, id: i64, agora: f64) {
        if let Some(i) = self.tarefas.iter().position(|t| t.id == id) {
            let mut t = self.tarefas.remove(i);
            t.coluna = Coluna::Concluido;
            t.motivo = None;
            self.abelha.concluiu(t.id, t.projeto_id, agora);
            self.tarefas.push(t);
        }
    }

    fn simular_erro(&mut self) {
        if let Some(t) = self.tarefas.iter_mut().find(|t| t.id == 103) {
            t.erro = Some("agente-6 parou: 3 testes falhando");
            t.erro_visto = false;
        }
        self.abelha.interromper_comemoracao();
    }

    fn simular_conclusao(&mut self, agora: f64) {
        let candidata =
            [Coluna::Revisao, Coluna::Backlog].into_iter().find_map(|c| self.tarefas.iter().find(|t| t.projeto_id == 1 && t.coluna == c).map(|t| t.id));
        if let Some(id) = candidata {
            self.concluir_demo(id, agora);
        }
    }

    fn simular_aprovacao(&mut self) {
        if let Some(t) = self.tarefas.iter_mut().find(|t| t.projeto_id == 3 && t.coluna == Coluna::Backlog) {
            t.coluna = Coluna::AguardandoVoce;
            t.motivo = Some("pede aprovação: merge na main");
        }
    }

    fn resolver_tudo(&mut self) {
        for t in &mut self.tarefas {
            t.erro = None;
            if t.coluna == Coluna::AguardandoVoce && t.id != 102 {
                t.coluna = Coluna::Backlog;
                t.motivo = None;
            }
        }
    }

    // Partes da tela

    fn topo(&mut self, ui: &mut egui::Ui, agora: f64) {
        let p = cores();
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            let perfil = self.perfil.as_ref().map(|p| p.nome.clone()).unwrap_or_default();
            match self.projeto_em_foco() {
                None => {
                    ui.label(texto_forte(&perfil, 15.0).color(p.texto));
                    ui.label(RichText::new("todos os projetos").color(p.suave));
                }
                Some(projeto) => {
                    ui.label(RichText::new(format!("{perfil}  ›  {}  ›", projeto.workspace)).color(p.suave));
                    ui.label(texto_forte(&projeto.nome, 15.0).color(p.texto));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.demo {
                    if let Some(aviso) = &self.aviso {
                        ui.label(RichText::new(aviso).color(p.erro).size(12.5));
                    }
                    return;
                }
                // Modo demonstração: cargas de teste e cenários simulados para a abelha.
                let modos = ["parada", "leve", "pesada"];
                let atual = modos.iter().position(|m| *m == self.carga).unwrap_or(0);
                if let Some(i) = tema::segmentado(ui, &["Parado", "Carga leve", "Carga pesada"], atual) {
                    self.carga = modos[i];
                    pedir_carga(modos[i]);
                }
                ui.add_space(8.0);
                let resposta = tema::chip(ui, "Simular", "", false);
                egui::Popup::menu(&resposta).show(|ui| {
                    ui.set_min_width(280.0);
                    if tema::opcao_menu(ui, "Erro no api-pedidos", false) {
                        self.simular_erro();
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Tarefa concluída no loja-web", false) {
                        self.simular_conclusao(agora);
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Os dois ao mesmo tempo", false) {
                        self.simular_erro();
                        self.simular_conclusao(agora);
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Pedido de aprovação no estudos-rust", false) {
                        self.simular_aprovacao();
                        ui.close();
                    }
                    if tema::opcao_menu(ui, "Resolver tudo", false) {
                        self.resolver_tudo();
                        ui.close();
                    }
                });
                ui.add_space(12.0);
                ui.label(RichText::new(format!("{} FPS · {} recebidos", self.fps, formatar_vazao(self.vazao))).color(p.suave).size(11.5));
            });
        });
    }

    /// Barra acima do quadro: filtros à esquerda, "Nova tarefa" à direita.
    fn barra_do_quadro(&mut self, ui: &mut egui::Ui) {
        // Pasta sem git não tem branches para filtrar.
        let sem_branches = self.projeto_em_foco().is_some_and(|p| p.sem_git);
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            let valor = self.filtro.clone().unwrap_or_else(|| "todas".into());
            let resposta = if sem_branches { None } else { Some(tema::chip(ui, "Branch", &valor, self.filtro.is_some())) };
            if let Some(resposta) = resposta {
                egui::Popup::menu(&resposta).show(|ui| {
                    ui.set_min_width(240.0);
                    if tema::opcao_menu(ui, "todas", self.filtro.is_none()) {
                        self.filtro = None;
                        ui.close();
                    }
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for b in &self.branches {
                            if tema::opcao_menu(ui, b, self.filtro.as_deref() == Some(b)) {
                                self.filtro = Some(b.clone());
                                ui.close();
                            }
                        }
                    });
                });
            }
            if self.demo {
                let quantidade = self.tarefas.len().to_string();
                let resposta = tema::chip(ui, "Cartões", &quantidade, false);
                egui::Popup::menu(&resposta).show(|ui| {
                    ui.set_min_width(200.0);
                    for n in [50, 500] {
                        if tema::opcao_menu(ui, &format!("{n} cartões"), self.tarefas.len() == n) {
                            self.tarefas = dados::gerar_demo(n);
                            ui.close();
                        }
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let projeto = self.projeto_em_foco().cloned();
                let pode = projeto.is_some() && !self.demo;
                let resposta = tema::botao_principal(ui, "+ Nova tarefa", pode);
                let resposta = if projeto.is_none() { resposta.on_hover_text("Escolha um projeto na barra lateral") } else { resposta };
                if resposta.clicked()
                    && let Some(projeto) = projeto
                {
                    self.dialogo = Some(Dialogo::NovaTarefa(dialogos::NovaTarefa::new(projeto)));
                }
            });
        });
        ui.add_space(10.0);
    }

    /// Perfil sem projeto: um convite para adicionar o primeiro.
    fn sem_projetos(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            ui.allocate_ui(egui::vec2(460.0, 0.0), |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(412.0);
                    tema::cabecalho(ui, "Nenhum projeto ainda", "Adicione um repositório git ou uma pasta de trabalho para começar a organizar as tarefas.");
                    ui.add_space(16.0);
                    if tema::botao_principal(ui, "Adicionar projeto", true).clicked()
                        && let Some(perfil) = &self.perfil
                    {
                        self.dialogo = Some(Dialogo::NovoProjeto(dialogos::NovoProjeto::new(perfil.id)));
                    }
                });
            });
        });
    }

    fn lateral(&mut self, ui: &mut egui::Ui, agora: f64, linha: &str) -> (Option<egui::Rect>, bool) {
        let p = cores();
        let rodando = !self.demo || self.carga != "parada";
        let (mut caixa_abelha, mut abelha_clicada) = (None, false);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
            tema::logo(ui.painter(), r.center(), 11.0);
            ui.label(texto_forte("Colmeia", 17.0).color(p.texto));
        });
        ui.add_space(18.0);

        // Perfil: trocar, criar ou sair.
        let nome = self.perfil.as_ref().map(|p| p.nome.clone()).unwrap_or_default();
        let resposta = tema::chip(ui, "Perfil", &nome, false);
        if !self.demo {
            let mut ir_para: Option<Tela> = None;
            let mut trocar: Option<api::Perfil> = None;
            egui::Popup::menu(&resposta).show(|ui| {
                ui.set_min_width(220.0);
                for outro in &self.perfis {
                    let atual = self.perfil.as_ref().is_some_and(|p| p.id == outro.id);
                    if tema::opcao_menu(ui, &outro.nome, atual) {
                        if !atual {
                            trocar = Some(outro.clone());
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if tema::opcao_menu(ui, "Criar perfil…", false) {
                    ir_para = Some(Tela::Entrada(Box::new(entrada::Entrada::new(true))));
                    ui.close();
                }
                if tema::opcao_menu(ui, "Sair do perfil", false) {
                    ir_para = Some(Tela::Entrada(Box::new(entrada::Entrada::new(false))));
                    ui.close();
                }
            });
            if let Some(perfil) = trocar {
                self.entrar(perfil, ui.ctx());
            }
            if let Some(tela) = ir_para {
                self.perfil = None;
                self.tela = tela;
                return (None, false);
            }
        }
        ui.add_space(14.0);

        if item_lateral(ui, "Todos os projetos", self.escopo == Escopo::Perfil, 10.0, None).clicked() {
            self.mudar_escopo(Escopo::Perfil);
        }
        ui.add_space(8.0);
        let mut workspace_anterior = String::new();
        let mut mudar = None;
        let mut remover = None;
        for projeto in &self.projetos {
            if projeto.workspace != workspace_anterior {
                ui.add_space(6.0);
                ui.label(RichText::new(&projeto.workspace).color(p.suave).size(11.5));
                workspace_anterior = projeto.workspace.clone();
            }
            let ativo = self.escopo == Escopo::Projeto(projeto.id);
            let estado = abelha::estado_base(self.tarefas.iter().filter(|t| t.projeto_id == projeto.id), rodando);
            let concluiu = self.abelha.conclusoes.iter().any(|c| c.projeto_id == projeto.id && agora - c.em < abelha::CONCLUSAO_RECENTE);
            let tem_erro = self.tarefas.iter().any(|t| t.projeto_id == projeto.id && t.erro.is_some());
            let resposta = item_lateral(ui, &projeto.nome, ativo, 10.0, abelha::cor_ponto(estado, tem_erro, concluiu));
            let resposta = if projeto.caminho.is_empty() { resposta } else { resposta.on_hover_text(&projeto.caminho) };
            if resposta.clicked() {
                mudar = Some(projeto.id);
            }
            if !self.demo {
                resposta.context_menu(|ui| {
                    ui.set_min_width(240.0);
                    if tema::opcao_menu(ui, "Remover da Colmeia…", false) {
                        remover = Some((projeto.id, projeto.nome.clone()));
                        ui.close();
                    }
                });
            }
        }
        if let Some(id) = mudar {
            self.mudar_escopo(Escopo::Projeto(id));
        }
        if let Some((id, nome)) = remover {
            self.dialogo = Some(Dialogo::RemoverProjeto { id, nome, erro: None });
        }
        ui.add_space(6.0);
        if !self.demo
            && item_lateral(ui, "+ Novo projeto", false, 10.0, None).clicked()
            && let Some(perfil) = &self.perfil
        {
            self.dialogo = Some(Dialogo::NovoProjeto(dialogos::NovoProjeto::new(perfil.id)));
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            let atual = tema::Escolha::TODAS.iter().position(|e| *e == self.tema).unwrap_or(0);
            let largura = ui.available_width();
            if let Some(i) = tema::segmentado_com_largura(ui, &tema::Escolha::TODAS.map(|e| e.nome()), atual, Some(largura)) {
                self.tema = tema::Escolha::TODAS[i];
                self.tema.aplicar(ui.ctx());
                if !self.demo
                    && let Some(perfil) = &mut self.perfil
                {
                    perfil.tema = self.tema.chave().into();
                    let _ = api::definir_tema(perfil.id, self.tema.chave());
                }
            }
            ui.add_space(10.0);
            // COLMEIA_SEM_ABELHA=1 desliga a abelha, como a opção por perfil.
            if !self.sem_abelha {
                let resposta = self.abelha.mostrar(ui, agora, linha);
                if resposta.clicked() {
                    self.abelha.resumo_aberto = !self.abelha.resumo_aberto;
                    abelha_clicada = true;
                }
                caixa_abelha = Some(resposta.rect);
            }
        });
        (caixa_abelha, abelha_clicada)
    }

    fn painel_tarefa(&mut self, ui: &mut egui::Ui, id: i64, foco: i64, agora: f64) {
        let Some(tarefa) = self.tarefas.iter().find(|t| t.id == id) else {
            self.tela = Tela::Quadro;
            return;
        };
        let agentes = tarefa.agentes.clone();
        let pasta = tarefa.pasta.clone();
        let p = cores();
        let (mut voltar, mut novo_agente, mut abrir_editor, mut abrir_pasta) = (false, false, false, false);
        ui.horizontal(|ui| {
            voltar = tema::botao_secundario(ui, "‹ Quadro").clicked();
            ui.add_space(8.0);
            ui.label(texto_forte(&tarefa.titulo, 16.0).color(p.texto));
            ui.label(RichText::new(format!("#{}", tarefa.id)).color(p.suave));
            if tarefa.branch.is_empty() {
                ui.label(RichText::new("pasta").color(p.suave).monospace());
            } else {
                ui.label(RichText::new(&tarefa.branch).color(p.destaque).monospace());
            }
            if tarefa.em_copia && !self.demo {
                ui.label(RichText::new("cópia isolada").color(p.suave)).on_hover_text(&tarefa.pasta);
            }
            ui.label(RichText::new(tarefa.coluna.nome()).color(p.ok));
            if let Some(erro) = tarefa.erro {
                ui.label(RichText::new(format!("erro: {erro}")).color(p.erro));
            }
            if self.demo {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                novo_agente = !agentes.is_empty() && tema::botao_principal(ui, "+ Agente", true).clicked();
                ui.add_space(6.0);
                abrir_pasta = tema::botao_secundario(ui, "Abrir pasta").on_hover_text(&pasta).clicked();
                if let Some((nome, _)) = self.editor {
                    ui.add_space(6.0);
                    abrir_editor = tema::botao_secundario(ui, &format!("Abrir no {nome}")).on_hover_text(&pasta).clicked();
                }
            });
        });
        ui.add_space(8.0);
        if abrir_editor
            && let Some((nome, comando)) = self.editor
            && let Err(e) = sistema::abrir_com(comando, &pasta)
        {
            self.avisar(format!("Não consegui abrir o {nome}: {e}"), agora);
        }
        if abrir_pasta && let Err(e) = sistema::abrir_pasta(&pasta) {
            self.avisar(format!("Não consegui abrir a pasta: {e}"), agora);
        }
        if novo_agente {
            self.adicionar_agente(id, false);
        }
        if voltar || ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.ctrl) {
            self.tela = Tela::Quadro;
            return;
        }
        if agentes.is_empty() {
            self.sem_agentes(ui, id, &pasta);
            return;
        }
        let foco = if agentes.iter().any(|a| a.id == foco) { foco } else { agentes[0].id };

        let area = ui.available_rect_before_wrap();
        let outros: Vec<AgenteTela> = agentes.iter().filter(|a| a.id != foco).cloned().collect();
        let em_foco = agentes.iter().find(|a| a.id == foco).cloned().expect("o foco é um dos agentes");
        let largura_lateral = if outros.is_empty() { 0.0 } else { (area.width() * 0.3).max(280.0) };
        let coluna = egui::Rect::from_min_max(area.min, egui::pos2(area.max.x - largura_lateral - if outros.is_empty() { 0.0 } else { 10.0 }, area.max.y));
        let principal = egui::Rect::from_min_max(coluna.min, egui::pos2(coluna.max.x, coluna.max.y - self.compositor.altura() - 10.0));
        let mut pedidos = vec![self.caixa_terminal(ui, principal, &em_foco, true, 13.0, "tempo real")];
        let caixa_mensagem = egui::Rect::from_min_max(egui::pos2(coluna.min.x, principal.max.y + 10.0), coluna.max);
        if let Some(envio) = self.compositor.mostrar(ui, caixa_mensagem, &agentes, foco) {
            for agente in envio.destinos {
                if let Some(t) = self.terminais.get(&agente) {
                    t.enviar(&envio.texto);
                }
            }
        }

        let altura = if outros.is_empty() { 0.0 } else { (area.height() - 10.0 * (outros.len() as f32 - 1.0)) / outros.len() as f32 };
        for (n, agente) in outros.iter().enumerate() {
            let min = egui::pos2(area.max.x - largura_lateral, area.min.y + n as f32 * (altura + 10.0));
            let caixa = egui::Rect::from_min_size(min, egui::vec2(largura_lateral, altura));
            pedidos.push(self.caixa_terminal(ui, caixa, agente, false, 10.0, "miniatura · clique para focar"));
        }
        for pedido in pedidos.into_iter().flatten() {
            match pedido {
                Pedido::Focar(agente) => self.tela = Tela::Tarefa { id, foco: agente },
                Pedido::Iniciar(agente) => self.iniciar_agente(agente, ui.ctx(), agora),
                Pedido::Remover(agente) => self.remover_agente(agente, agora),
            }
        }
    }

    /// Tarefa sem agentes: abrir um novo ou retomar uma conversa do Claude Code
    /// começada em outro lugar (no IntelliJ, por exemplo) na mesma pasta.
    fn sem_agentes(&mut self, ui: &mut egui::Ui, id: i64, pasta: &str) {
        let tem_claude = self.opcoes_de_agente().iter().any(|o| o.id == "claude");
        let (mut novo, mut retomar) = (false, false);
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.allocate_ui(egui::vec2(520.0, 0.0), |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(472.0);
                    let texto = if self.demo {
                        "Na demonstração, os agentes são terminais de teste.".to_string()
                    } else {
                        format!("O agente abre em {pasta}, com a conta de IA do perfil. Se você já conversava com o Claude Code nessa pasta, dá para continuar a mesma conversa aqui.")
                    };
                    tema::cabecalho(ui, "Esta tarefa ainda não tem agentes", &texto);
                    if self.demo {
                        return;
                    }
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        novo = tema::botao_principal(ui, "Adicionar agente", true).clicked();
                        if tem_claude {
                            ui.add_space(8.0);
                            retomar = tema::botao_secundario(ui, "Retomar conversa do Claude Code").clicked();
                        }
                    });
                });
            });
        });
        if novo || retomar {
            self.adicionar_agente(id, retomar);
        }
    }

    /// Desenha o cartão de um agente com cabeçalho e terminal. Um agente parado
    /// mostra o que ficou na tela e o botão para iniciar de novo.
    fn caixa_terminal(&mut self, ui: &mut egui::Ui, caixa: egui::Rect, agente: &AgenteTela, focado: bool, fonte: f32, ritmo: &str) -> Option<Pedido> {
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa));
        let mut pedido = None;
        let demo = self.demo;
        let parado = self.terminais.get(&agente.id).is_none_or(|t| t.encerrado());
        egui::Frame::new()
            .fill(cores().superficie_alta)
            .stroke(Stroke::new(if focado { 1.5 } else { 1.0 }, if focado { cores().destaque } else { cores().borda }))
            .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::symmetric(0, 6))
            .show(&mut filho, |ui| {
                ui.set_min_size(caixa.size() - egui::vec2(2.0, 2.0));
                let cabecalho = ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    let (ponto, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(ponto.center(), 3.5, if parado { cores().suave } else { cores().ok });
                    ui.label(texto_forte(agente.nome(), 13.5).color(cores().texto));
                    ui.label(RichText::new(format!("· {}", agente.papel)).color(cores().suave));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(RichText::new(if parado { "parado" } else { ritmo }).color(cores().suave).small());
                    });
                });
                if !demo {
                    cabecalho.response.interact(egui::Sense::click()).context_menu(|ui| {
                        ui.set_min_width(220.0);
                        if tema::opcao_menu(ui, "Parar e remover agente", false) {
                            pedido = Some(Pedido::Remover(agente.id));
                            ui.close();
                        }
                    });
                }
                match self.terminais.get_mut(&agente.id) {
                    Some(t) => {
                        if t.mostrar(ui, fonte).clicked() && !focado {
                            pedido = Some(Pedido::Focar(agente.id));
                        }
                    }
                    None => {
                        ui.allocate_space(ui.available_size());
                    }
                }
                if parado && !demo {
                    let centro = ui.min_rect().center();
                    let rotulo = if agente.ferramenta == "claude" { "Retomar conversa" } else { "Iniciar de novo" };
                    let area = egui::Rect::from_center_size(centro, egui::vec2(220.0, 40.0));
                    let mut botao = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Center)));
                    if tema::botao_principal(&mut botao, rotulo, true).clicked() {
                        pedido = Some(Pedido::Iniciar(agente.id));
                    }
                }
            });
        pedido
    }

    /// Aviso passageiro no rodapé da tela, some sozinho depois de 6 segundos.
    fn mostrar_recado(&mut self, ctx: &egui::Context, agora: f64) {
        let Some((texto, desde)) = &self.recado else {
            return;
        };
        if agora - desde > 6.0 {
            self.recado = None;
            return;
        }
        let p = cores();
        egui::Area::new(egui::Id::new("recado")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0]).show(ctx, |ui| {
            egui::Frame::new().fill(p.superficie_alta).stroke(Stroke::new(1.0, p.erro)).corner_radius(12).inner_margin(egui::Margin::symmetric(16, 10)).show(
                ui,
                |ui| {
                    ui.label(RichText::new(texto).color(p.texto));
                },
            );
        });
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
}

/// O que o cartão de um agente pede ao painel da tarefa.
enum Pedido {
    Focar(i64),
    Iniciar(i64),
    Remover(i64),
}

/// Item da barra lateral: linha inteira clicável, fundo ao passar o mouse e um
/// ponto opcional na cor do estado do projeto.
fn item_lateral(ui: &mut egui::Ui, texto: &str, ativo: bool, recuo: f32, ponto: Option<Color32>) -> egui::Response {
    let p = cores();
    let (rect, resposta) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ativo || resposta.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_CONTROLE), if ativo { p.realce } else { p.realce.gamma_multiply(0.6) });
    }
    let fonte = if ativo { tema::forte(13.5) } else { egui::FontId::proportional(13.5) };
    ui.painter().text(rect.left_center() + egui::vec2(recuo, 0.0), egui::Align2::LEFT_CENTER, texto, fonte, if ativo { p.texto } else { p.suave });
    if let Some(cor) = ponto {
        ui.painter().circle_filled(rect.right_center() - egui::vec2(14.0, 0.0), 4.0, cor);
    }
    resposta
}

fn formatar_vazao(b: u64) -> String {
    match b {
        b if b >= 1024 * 1024 => format!("{:.1} MB/s", b as f64 / 1048576.0),
        b if b >= 1024 => format!("{} KB/s", b / 1024),
        b => format!("{b} B/s"),
    }
}

impl eframe::App for Colmeia {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let agora = ui.input(|i| i.time);
        self.medir(agora);
        let p = cores();

        // Sem perfil: só a tela de entrada, sobre o favo.
        if let Tela::Entrada(entrada) = &mut self.tela {
            let mut escolhido = None;
            egui::CentralPanel::default().frame(egui::Frame::new().fill(p.fundo)).show(ui, |ui| {
                self.favo.desenhar(ui.painter(), ui.max_rect());
                egui::ScrollArea::vertical().show(ui, |ui| escolhido = entrada.mostrar(ui));
            });
            if let Some(perfil) = escolhido {
                self.entrar(perfil, ui.ctx());
            }
            return;
        }
        self.ajustar_ritmos();
        // O ponto de cada agente (no cartão e na abelha) segue o terminal dele.
        if !self.demo {
            for t in &mut self.tarefas {
                for a in &mut t.agentes {
                    a.ativo = self.terminais.get(&a.id).is_some_and(|t| !t.encerrado());
                }
            }
        }

        // Abrir a tarefa conta como ver o erro: a abelha para de ficar bugada.
        if let Tela::Tarefa { id, .. } = self.tela
            && let Some(t) = self.tarefas.iter_mut().find(|t| t.id == id)
        {
            t.erro_visto = true;
        }

        // Na demonstração, "rodando" é a carga de teste; no uso normal, os agentes ativos.
        let rodando = !self.demo || self.carga != "parada";
        let escopo = self.escopo;
        let base = abelha::estado_base(self.tarefas.iter().filter(|t| escopo.contem(t.projeto_id)), rodando);
        let estado = self.abelha.atualizar(base, |p| escopo.contem(p), agora);
        let no_escopo = || self.tarefas.iter().filter(|t| escopo.contem(t.projeto_id));
        let plural = |n: usize, um: &str, varios: &str| {
            if n == 1 { format!("1 {um}") } else { format!("{n} {varios}") }
        };
        let linha = match estado {
            Estado::Bugado => plural(no_escopo().filter(|t| t.erro.is_some() && !t.erro_visto).count(), "erro", "erros"),
            Estado::Aguardando => plural(no_escopo().filter(|t| t.coluna == Coluna::AguardandoVoce).count(), "tarefa esperando", "tarefas esperando"),
            Estado::Comemorando => "tarefa concluída".to_string(),
            Estado::Trabalhando => {
                plural(no_escopo().filter(|t| t.coluna == Coluna::Trabalhando && !t.agentes.is_empty()).count(), "tarefa em andamento", "tarefas em andamento")
            }
            Estado::Dormindo => "tudo quieto".to_string(),
        };

        let mut abelha = (None, false);
        egui::Panel::left("lateral")
            .exact_size(236.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.lateral).inner_margin(egui::Margin::symmetric(14, 18)))
            .show(ui, |ui| abelha = self.lateral(ui, agora, &linha));
        if matches!(self.tela, Tela::Entrada(_)) {
            return;
        }
        let (caixa_abelha, abelha_clicada) = abelha;

        egui::Panel::top("topo")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.fundo).inner_margin(egui::Margin::symmetric(20, 10)))
            .show(ui, |ui| self.topo(ui, agora));

        let mut acoes = Vec::new();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.fundo).inner_margin(egui::Margin { left: 20, right: 20, top: 6, bottom: 16 })).show(
            ui,
            |ui| {
                let area = ui.max_rect().expand2(egui::vec2(20.0, 16.0));
                self.favo.desenhar(ui.painter(), area);
                match self.tela {
                    Tela::Quadro if self.projetos.is_empty() => self.sem_projetos(ui),
                    Tela::Quadro => {
                        self.barra_do_quadro(ui);
                        let projeto = match self.escopo {
                            Escopo::Perfil => None,
                            Escopo::Projeto(id) => Some(id),
                        };
                        acoes = quadro::mostrar(ui, &mut self.tarefas, projeto, self.filtro.as_deref(), &self.terminais);
                    }
                    Tela::Tarefa { id, foco } => self.painel_tarefa(ui, id, foco, agora),
                    Tela::Entrada(_) => {}
                }
            },
        );
        for acao in acoes {
            match acao {
                quadro::Acao::AbrirTarefa(id) => self.abrir_tarefa(id),
                quadro::Acao::Moveu(id, coluna) => self.mover(id, coluna, agora),
                quadro::Acao::Remover(id) if self.demo => self.tarefas.retain(|t| t.id != id),
                quadro::Acao::Remover(id) => {
                    if let Some(t) = self.tarefas.iter().find(|t| t.id == id) {
                        self.dialogo = Some(Dialogo::RemoverTarefa { id, titulo: t.titulo.clone(), copia: t.em_copia, erro: None });
                    }
                }
            }
        }

        if let Some(dialogo) = &mut self.dialogo {
            let perfil = self.perfil.as_ref().map_or(0, |p| p.id);
            match dialogo.mostrar(ui.ctx(), perfil) {
                Resultado::Continua => {}
                Resultado::Fechar => self.dialogo = None,
                Resultado::Mudou => {
                    self.dialogo = None;
                    self.recarregar(agora);
                    self.carregar_branches();
                    self.conectar_agentes(ui.ctx());
                }
                Resultado::ProjetoCriado(id) => {
                    self.dialogo = None;
                    self.recarregar(agora);
                    self.mudar_escopo(Escopo::Projeto(id));
                }
                Resultado::AgenteCriado(agente) => {
                    self.dialogo = None;
                    self.agente_criado(agente, ui.ctx(), agora);
                }
            }
        }

        if self.abelha.resumo_aberto
            && let Some(caixa) = caixa_abelha
        {
            let ancora = egui::pos2(caixa.right() + 20.0, caixa.bottom());
            let (clicada, area) = abelha::resumo(ui.ctx(), ancora, &self.tarefas, |p| escopo.contem(p), &self.abelha.conclusoes, rodando, agora);
            if let Some(id) = clicada {
                self.abrir_tarefa(id);
            } else if area.clicked_elsewhere() && !abelha_clicada {
                self.abelha.resumo_aberto = false;
            }
        }
        self.mostrar_recado(ui.ctx(), agora);
    }
}

fn main() -> eframe::Result {
    // O núcleo é iniciado antes da janela; se não der, a tela abre e mostra o motivo.
    let aviso = canal::garantir_nucleo().err().map(|e| format!("Núcleo: {e}"));
    if let Some(a) = &aviso {
        eprintln!("{a}");
    }
    let opcoes =
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title("Colmeia").with_inner_size([1600.0, 900.0]), ..Default::default() };
    eframe::run_native("colmeia", opcoes, Box::new(move |cc| Ok(Box::new(Colmeia::new(cc, aviso)))))
}
