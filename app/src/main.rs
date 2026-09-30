//! Tela da Colmeia: quadro de tarefas do projeto, painel da tarefa (só o
//! terminal em foco é tempo real, com caixa de mensagem para os agentes) e a
//! abelha da barra lateral, que resume o que mais precisa de você. Fala com o
//! núcleo em Go pelo canal local (socket Unix + token).

mod abelha;
mod canal;
mod dados;
mod quadro;
mod tema;
mod terminal;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};
use abelha::Abelha;
use dados::{BRANCHES, Coluna, PROJETOS, Tarefa};
use mascote::Estado;
use tema::{cores, texto_forte};
use terminal::{MINIATURA, SO_CARTAO, TEMPO_REAL, TerminalAgente, pedir_carga};

pub const PAPEIS: [&str; 10] = ["líder", "dev", "dev", "revisor", "testador", "dev", "dev", "revisor", "testador", "dev"];

/// O que está na tela: o perfil inteiro ou um projeto. A abelha resume o escopo.
#[derive(Clone, Copy, PartialEq)]
enum Escopo {
    Perfil,
    Projeto(&'static str),
}

impl Escopo {
    fn contem(self, projeto: &str) -> bool {
        match self {
            Escopo::Perfil => true,
            Escopo::Projeto(p) => p == projeto,
        }
    }
}

enum Tela {
    Quadro,
    Tarefa { id: u32, foco: usize },
}

struct Colmeia {
    terminais: Vec<TerminalAgente>,
    tarefas: Vec<Tarefa>,
    tela: Tela,
    escopo: Escopo,
    abelha: Abelha,
    sem_abelha: bool,
    tema: tema::Escolha,
    favo: tema::Favo,
    /// Caixa de mensagem do painel da tarefa.
    rascunho: String,
    para_todos: bool,
    focar_compositor: bool,
    /// O núcleo está em modo demonstração (cargas de teste e cenários simulados).
    demo: bool,
    /// Problema com o núcleo, mostrado no topo da tela.
    aviso: Option<String>,
    filtro: Option<&'static str>,
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
        // COLMEIA_TEMA=claro | escuro | sistema escolhe o tema inicial.
        let escolha = match std::env::var("COLMEIA_TEMA").as_deref() {
            Ok("claro") => tema::Escolha::Claro,
            Ok("sistema") => tema::Escolha::Sistema,
            _ => tema::Escolha::Escuro,
        };
        escolha.aplicar(&cc.egui_ctx);
        let bytes = Arc::new(AtomicU64::new(0));
        let terminais = (0..PAPEIS.len())
            .map(|id| TerminalAgente::conectar(id, cc.egui_ctx.clone(), bytes.clone(), SO_CARTAO))
            .collect();
        // Estado inicial por variável de ambiente, para medir sem clicar:
        // COLMEIA_CARTOES=500 e COLMEIA_TAREFA=101 (abre o painel da tarefa).
        let cartoes = std::env::var("COLMEIA_CARTOES").ok().and_then(|v| v.parse().ok()).unwrap_or(50);
        let tarefas = dados::gerar(cartoes);
        let tela = std::env::var("COLMEIA_TAREFA")
            .ok()
            .and_then(|v| v.parse().ok())
            .and_then(|id: u32| tarefas.iter().find(|t| t.id == id))
            .map_or(Tela::Quadro, |t| Tela::Tarefa { id: t.id, foco: t.agentes.first().copied().unwrap_or(0) });
        // COLMEIA_CENARIO=erro já abre com o erro no api-pedidos, para medir a abelha bugada.
        let mut tarefas = tarefas;
        if std::env::var("COLMEIA_CENARIO").is_ok_and(|v| v == "erro")
            && let Some(t) = tarefas.iter_mut().find(|t| t.id == 103)
        {
            t.erro = Some("agente-6 parou: 3 testes falhando");
        }
        Self {
            terminais,
            tarefas,
            tela,
            escopo: Escopo::Projeto("loja-web"),
            abelha: Abelha::new(),
            sem_abelha: std::env::var("COLMEIA_SEM_ABELHA").is_ok_and(|v| v == "1"),
            tema: escolha,
            favo: tema::Favo::default(),
            rascunho: String::new(),
            para_todos: false,
            focar_compositor: true,
            demo: canal::pedir("GET", "/v1/versao").is_ok_and(|v| v.contains("\"demo\":true")),
            aviso,
            filtro: None,
            carga: "parada",
            bytes,
            quadros: 0,
            ultimo_segundo: 0.0,
            bytes_antes: 0,
            quadros_antes: 0,
            fps: 0,
            vazao: 0,
        }
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

    /// Cada terminal recebe no ritmo que a tela precisa: tempo real só para o
    /// que está em foco, miniatura para os outros da tarefa, e o mínimo para
    /// quem só aparece como última linha num cartão.
    fn ajustar_ritmos(&self) {
        let (foco, tarefa) = match self.tela {
            Tela::Quadro => (None, None),
            Tela::Tarefa { id, foco } => (Some(foco), self.tarefas.iter().find(|t| t.id == id)),
        };
        for (i, t) in self.terminais.iter().enumerate() {
            let ms = if Some(i) == foco {
                TEMPO_REAL
            } else if tarefa.is_some_and(|t| t.agentes.contains(&i)) {
                MINIATURA
            } else {
                SO_CARTAO
            };
            t.definir_intervalo(ms);
        }
    }

    fn abrir_tarefa(&mut self, id: u32) {
        let Some(t) = self.tarefas.iter().find(|t| t.id == id) else { return };
        if !self.escopo.contem(t.projeto) {
            self.escopo = Escopo::Projeto(t.projeto);
        }
        self.tela = Tela::Tarefa { id, foco: t.agentes.first().copied().unwrap_or(0) };
        self.abelha.resumo_aberto = false;
        self.focar_compositor = true;
    }

    fn concluir(&mut self, id: u32, agora: f64) {
        if let Some(i) = self.tarefas.iter().position(|t| t.id == id) {
            let mut t = self.tarefas.remove(i);
            t.coluna = Coluna::Concluido;
            t.motivo = None;
            self.abelha.concluiu(t.id, t.projeto, agora);
            self.tarefas.push(t);
        }
    }

    // Cenários para testar a abelha sem esperar um erro de verdade.
    fn simular_erro(&mut self) {
        if let Some(t) = self.tarefas.iter_mut().find(|t| t.id == 103) {
            t.erro = Some("agente-6 parou: 3 testes falhando");
            t.erro_visto = false;
        }
        self.abelha.interromper_comemoracao();
    }

    fn simular_conclusao(&mut self, agora: f64) {
        let candidata = [Coluna::Revisao, Coluna::Backlog].into_iter().find_map(|c| {
            self.tarefas.iter().find(|t| t.projeto == "loja-web" && t.coluna == c).map(|t| t.id)
        });
        if let Some(id) = candidata {
            self.concluir(id, agora);
        }
    }

    fn simular_aprovacao(&mut self) {
        if let Some(t) = self.tarefas.iter_mut().find(|t| t.projeto == "estudos-rust" && t.coluna == Coluna::Backlog) {
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

    fn topo(&mut self, ui: &mut egui::Ui, agora: f64) {
        let p = cores();
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            match self.escopo {
                Escopo::Perfil => {
                    ui.label(texto_forte("Profissional", 15.0).color(p.texto));
                    ui.label(RichText::new("todos os projetos").color(p.suave));
                }
                Escopo::Projeto(projeto) => {
                    ui.label(RichText::new("Profissional  ›  Empresa X  ›").color(p.suave));
                    ui.label(texto_forte(projeto, 15.0).color(p.texto));
                }
            }
            ui.add_space(16.0);

            // Filtros em chips: ficam destacados quando estão em uso.
            let resposta = tema::chip(ui, "Branch", self.filtro.unwrap_or("todas"), self.filtro.is_some());
            egui::Popup::menu(&resposta).show(|ui| {
                ui.set_min_width(210.0);
                if ui.selectable_label(self.filtro.is_none(), "todas").clicked() {
                    self.filtro = None;
                    ui.close();
                }
                for b in BRANCHES {
                    if ui.selectable_label(self.filtro == Some(b), b).clicked() {
                        self.filtro = Some(b);
                        ui.close();
                    }
                }
            });
            let quantidade = self.tarefas.len().to_string();
            let resposta = tema::chip(ui, "Cartões", &quantidade, false);
            egui::Popup::menu(&resposta).show(|ui| {
                for n in [50, 500] {
                    if ui.selectable_label(self.tarefas.len() == n, format!("{n} cartões")).clicked() {
                        self.tarefas = dados::gerar(n);
                        ui.close();
                    }
                }
            });

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
                    ui.set_min_width(260.0);
                    if ui.button("Erro no api-pedidos").clicked() {
                        self.simular_erro();
                        ui.close();
                    }
                    if ui.button("Tarefa concluída no loja-web").clicked() {
                        self.simular_conclusao(agora);
                        ui.close();
                    }
                    if ui.button("Os dois ao mesmo tempo").clicked() {
                        self.simular_erro();
                        self.simular_conclusao(agora);
                        ui.close();
                    }
                    if ui.button("Pedido de aprovação no estudos-rust").clicked() {
                        self.simular_aprovacao();
                        ui.close();
                    }
                    if ui.button("Resolver tudo").clicked() {
                        self.resolver_tudo();
                        ui.close();
                    }
                });
                ui.add_space(12.0);
                ui.label(RichText::new(format!("{} FPS · {} recebidos", self.fps, formatar_vazao(self.vazao))).color(p.suave).size(11.5));
            });
        });
    }

    fn painel_tarefa(&mut self, ui: &mut egui::Ui, id: u32, foco: usize) {
        let Some(tarefa) = self.tarefas.iter().find(|t| t.id == id) else {
            self.tela = Tela::Quadro;
            return;
        };
        let agentes = tarefa.agentes.clone();
        let mut voltar = false;
        ui.horizontal(|ui| {
            voltar = tema::botao_secundario(ui, "‹ Quadro").clicked();
            ui.add_space(8.0);
            ui.label(texto_forte(&tarefa.titulo, 16.0).color(cores().texto));
            ui.label(RichText::new(format!("#{}", tarefa.id)).color(cores().suave));
            ui.label(RichText::new(tarefa.branch).color(cores().destaque).monospace());
            ui.label(RichText::new(tarefa.coluna.nome()).color(cores().ok));
            if let Some(erro) = tarefa.erro {
                ui.label(RichText::new(format!("erro: {erro}")).color(Color32::from_rgb(0xf2, 0x5b, 0x6b)));
            }
        });
        ui.add_space(8.0);
        if voltar || ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.ctrl) {
            self.tela = Tela::Quadro;
            return;
        }
        if agentes.is_empty() {
            ui.label(RichText::new("Nenhum agente nesta tarefa ainda.").color(cores().suave));
            return;
        }

        let area = ui.available_rect_before_wrap();
        let outros: Vec<usize> = agentes.iter().copied().filter(|&a| a != foco).collect();
        let largura_lateral = if outros.is_empty() { 0.0 } else { (area.width() * 0.3).max(280.0) };
        let coluna = egui::Rect::from_min_max(area.min, egui::pos2(area.max.x - largura_lateral - if outros.is_empty() { 0.0 } else { 10.0 }, area.max.y));
        // A caixa de mensagem cresce com o texto, até 5 linhas.
        let linhas = (self.rascunho.split('\n').count()).clamp(1, 5) as f32;
        let altura_compositor = 54.0 + 19.0 * (linhas - 1.0) + 24.0;
        let principal = egui::Rect::from_min_max(coluna.min, egui::pos2(coluna.max.x, coluna.max.y - altura_compositor - 10.0));
        self.caixa_terminal(ui, principal, foco, true, 13.0, "tempo real");
        let caixa_mensagem = egui::Rect::from_min_max(egui::pos2(coluna.min.x, principal.max.y + 10.0), coluna.max);
        self.compositor(ui, caixa_mensagem, &agentes, foco);

        let altura = if outros.is_empty() { 0.0 } else { (area.height() - 10.0 * (outros.len() as f32 - 1.0)) / outros.len() as f32 };
        for (n, &agente) in outros.iter().enumerate() {
            let min = egui::pos2(area.max.x - largura_lateral, area.min.y + n as f32 * (altura + 10.0));
            let caixa = egui::Rect::from_min_size(min, egui::vec2(largura_lateral, altura));
            if self.caixa_terminal(ui, caixa, agente, false, 10.0, "miniatura · clique para focar") {
                self.tela = Tela::Tarefa { id, foco: agente };
            }
        }
    }

    /// Caixa de mensagem: escreve para o agente em foco (ou todos da tarefa) e envia
    /// com Enter; Shift+Enter quebra a linha.
    fn compositor(&mut self, ui: &mut egui::Ui, area: egui::Rect, agentes: &[usize], foco: usize) {
        let p = cores();
        let id = egui::Id::new("compositor");
        if std::mem::take(&mut self.focar_compositor) {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let com_foco = ui.memory(|m| m.has_focus(id));
        // O Shift é lido no próprio evento do Enter: numa digitação rápida ele pode
        // já ter sido solto quando o quadro termina.
        let enviar_com_enter = com_foco
            && ui.input(|i| {
                i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } if !modifiers.shift))
            });

        let caixa = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), area.height() - 24.0));
        let contorno = if com_foco { egui::Stroke::new(1.5, p.destaque) } else { egui::Stroke::new(1.0, p.borda) };
        ui.painter().rect(caixa, CornerRadius::same(18), p.superficie_alta, contorno, egui::StrokeKind::Inside);

        let mut enviar = enviar_com_enter;
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa.shrink2(egui::vec2(10.0, 10.0))).layout(egui::Layout::left_to_right(egui::Align::Min)));
        let destino = if self.para_todos { format!("todos ({})", agentes.len()) } else { format!("agente-{foco} · {}", PAPEIS[foco]) };
        let resposta = tema::chip(&mut filho, "Para", &destino, self.para_todos);
        egui::Popup::menu(&resposta).show(|ui| {
            ui.set_min_width(240.0);
            if ui.selectable_label(!self.para_todos, format!("agente-{foco} ({}, em foco)", PAPEIS[foco])).clicked() {
                self.para_todos = false;
                ui.close();
            }
            if ui.selectable_label(self.para_todos, format!("Todos os {} agentes da tarefa", agentes.len())).clicked() {
                self.para_todos = true;
                ui.close();
            }
        });
        filho.add_space(6.0);
        filho.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if tema::botao_principal(ui, "Enviar", !self.rascunho.trim().is_empty()).clicked() {
                enviar = true;
            }
            ui.add_space(8.0);
            let dica = if self.para_todos { "Mensagem para todos os agentes da tarefa".to_string() } else { format!("Mensagem para agente-{foco}") };
            let campo = egui::TextEdit::multiline(&mut self.rascunho)
                .id(id)
                .frame(egui::Frame::NONE)
                .desired_rows(1)
                .desired_width(ui.available_width())
                .font(egui::FontId::proportional(14.0))
                .margin(egui::Margin::symmetric(4, 7))
                .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter))
                .hint_text(dica);
            ui.add(campo);
        });

        ui.painter().text(
            egui::pos2(caixa.left() + 14.0, caixa.bottom() + 12.0),
            egui::Align2::LEFT_CENTER,
            "Enter envia · Shift+Enter quebra a linha · clique no terminal para digitar direto nele",
            egui::FontId::proportional(11.5),
            p.suave,
        );

        let texto = self.rascunho.trim().to_string();
        if enviar && !texto.is_empty() {
            let destinos: Vec<usize> = if self.para_todos { agentes.to_vec() } else { vec![foco] };
            for agente in destinos {
                self.terminais[agente].enviar(&texto);
            }
            self.rascunho.clear();
            ui.memory_mut(|m| m.request_focus(id));
        }
    }

    /// Desenha um cartão com cabeçalho e terminal. Retorna se foi clicado.
    fn caixa_terminal(&mut self, ui: &mut egui::Ui, caixa: egui::Rect, agente: usize, focado: bool, fonte: f32, ritmo: &str) -> bool {
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa));
        let mut clicado = false;
        egui::Frame::new()
            .fill(cores().superficie_alta)
            .stroke(Stroke::new(if focado { 1.5 } else { 1.0 }, if focado { cores().destaque } else { cores().borda }))
            .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::symmetric(0, 6))
            .show(&mut filho, |ui| {
                ui.set_min_size(caixa.size() - egui::vec2(2.0, 2.0));
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    let (ponto, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(ponto.center(), 3.5, cores().ok);
                    ui.label(texto_forte(format!("agente-{agente}"), 13.5).color(cores().texto));
                    ui.label(RichText::new(format!("· {}", PAPEIS[agente])).color(cores().suave));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(RichText::new(ritmo).color(cores().suave).small());
                    });
                });
                clicado = self.terminais[agente].mostrar(ui, fonte).clicked();
            });
        clicado
    }
}

/// Item da barra lateral: linha inteira clicável, fundo ao passar o mouse e um
/// ponto opcional na cor do estado do projeto.
fn item_lateral(ui: &mut egui::Ui, texto: &str, ativo: bool, recuo: f32, ponto: Option<Color32>) -> bool {
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
    resposta.clicked()
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
        tema::sincronizar(ui.ctx());
        self.medir(agora);
        self.ajustar_ritmos();

        // Abrir a tarefa conta como ver o erro: a abelha para de ficar bugada.
        if let Tela::Tarefa { id, .. } = self.tela
            && let Some(t) = self.tarefas.iter_mut().find(|t| t.id == id)
        {
            t.erro_visto = true;
        }

        let rodando = self.carga != "parada";
        let escopo = self.escopo;
        let base = abelha::estado_base(self.tarefas.iter().filter(|t| escopo.contem(t.projeto)), rodando);
        let estado = self.abelha.atualizar(base, |p| escopo.contem(p), agora);
        let no_escopo = || self.tarefas.iter().filter(|t| escopo.contem(t.projeto));
        let plural = |n: usize, um: &str, varios: &str| if n == 1 { format!("1 {um}") } else { format!("{n} {varios}") };
        let linha = match estado {
            Estado::Bugado => plural(no_escopo().filter(|t| t.erro.is_some() && !t.erro_visto).count(), "erro", "erros"),
            Estado::Aguardando => plural(no_escopo().filter(|t| t.coluna == Coluna::AguardandoVoce).count(), "tarefa esperando", "tarefas esperando"),
            Estado::Comemorando => "tarefa concluída".to_string(),
            Estado::Trabalhando => plural(
                no_escopo().filter(|t| t.coluna == Coluna::Trabalhando && !t.agentes.is_empty()).count(),
                "tarefa em andamento",
                "tarefas em andamento",
            ),
            Estado::Dormindo => "tudo quieto".to_string(),
        };

        let mut caixa_abelha = None;
        let mut abelha_clicada = false;
        let p = cores();
        egui::Panel::left("lateral")
            .exact_size(224.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.lateral).inner_margin(egui::Margin::symmetric(14, 18)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                    tema::logo(ui.painter(), r.center(), 11.0);
                    ui.label(texto_forte("Colmeia", 17.0).color(p.texto));
                });
                ui.add_space(22.0);
                ui.label(RichText::new("Perfil").color(p.suave).size(11.5));
                if item_lateral(ui, "Profissional", self.escopo == Escopo::Perfil, 10.0, None) {
                    self.escopo = Escopo::Perfil;
                    self.tela = Tela::Quadro;
                }
                ui.add_space(14.0);
                ui.label(RichText::new("Empresa X · projetos").color(p.suave).size(11.5));
                for projeto in PROJETOS {
                    let ativo = self.escopo == Escopo::Projeto(projeto);
                    let estado_projeto = abelha::estado_base(self.tarefas.iter().filter(|t| t.projeto == projeto), rodando);
                    let concluiu = self.abelha.conclusoes.iter().any(|c| c.projeto == projeto && agora - c.em < abelha::CONCLUSAO_RECENTE);
                    let tem_erro = self.tarefas.iter().any(|t| t.projeto == projeto && t.erro.is_some());
                    let ponto = abelha::cor_ponto(estado_projeto, tem_erro, concluiu);
                    if item_lateral(ui, projeto, ativo, 10.0, ponto) {
                        self.escopo = Escopo::Projeto(projeto);
                        self.tela = Tela::Quadro;
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("   + Novo projeto").color(p.destaque).size(13.0));

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    let atual = tema::Escolha::TODAS.iter().position(|e| *e == self.tema).unwrap_or(0);
                    let nomes = tema::Escolha::TODAS.map(|e| e.nome());
                    if let Some(i) = tema::segmentado(ui, &nomes, atual) {
                        self.tema = tema::Escolha::TODAS[i];
                        self.tema.aplicar(ui.ctx());
                    }
                    ui.add_space(10.0);
                    // COLMEIA_SEM_ABELHA=1 desliga a abelha, como a opção por perfil.
                    if !self.sem_abelha {
                        let resposta = self.abelha.mostrar(ui, agora, &linha);
                        if resposta.clicked() {
                            self.abelha.resumo_aberto = !self.abelha.resumo_aberto;
                            abelha_clicada = true;
                        }
                        caixa_abelha = Some(resposta.rect);
                    }
                });
            });

        egui::Panel::top("topo")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.fundo).inner_margin(egui::Margin::symmetric(20, 10)))
            .show(ui, |ui| self.topo(ui, agora));

        let mut acoes = Vec::new();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.fundo).inner_margin(egui::Margin { left: 20, right: 20, top: 6, bottom: 16 }))
            .show(ui, |ui| {
                let area = ui.max_rect().expand2(egui::vec2(20.0, 16.0));
                self.favo.desenhar(ui.painter(), area);
                match self.tela {
                Tela::Quadro => {
                    let projeto = match self.escopo {
                        Escopo::Perfil => None,
                        Escopo::Projeto(p) => Some(p),
                    };
                    acoes = quadro::mostrar(ui, &mut self.tarefas, projeto, self.filtro, &self.terminais);
                }
                Tela::Tarefa { id, foco } => self.painel_tarefa(ui, id, foco),
                }
            });
        for acao in acoes {
            match acao {
                quadro::Acao::AbrirTarefa(id) => self.abrir_tarefa(id),
                quadro::Acao::Moveu(id, Coluna::Concluido) => {
                    if let Some(t) = self.tarefas.iter().find(|t| t.id == id) {
                        self.abelha.concluiu(id, t.projeto, agora);
                    }
                }
                quadro::Acao::Moveu(..) => {}
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
    }
}

fn main() -> eframe::Result {
    // O núcleo é iniciado antes da janela; se não der, a tela abre e mostra o motivo.
    let aviso = canal::garantir_nucleo().err().map(|e| format!("Núcleo: {e}"));
    if let Some(a) = &aviso {
        eprintln!("{a}");
    }
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Colmeia").with_inner_size([1600.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native("colmeia", opcoes, Box::new(move |cc| Ok(Box::new(Colmeia::new(cc, aviso)))))
}
