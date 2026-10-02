//! A abelha da barra lateral: resume o que mais precisa de você no escopo que
//! está na tela (o perfil inteiro ou um projeto) e só anima quando o estado muda.

use std::time::Duration;

use eframe::egui::{self, Color32, Id, Order, Pos2, RichText, Sense, vec2};
use mascote::{Estado, Tempo};

use crate::dados::{Coluna, Tarefa};
use crate::tema::cores;

/// Quanto dura a comemoração de uma tarefa concluída.
pub const COMEMORACAO: f64 = 3.0;
/// Por quanto tempo uma conclusão continua marcada no ponto do projeto.
pub const CONCLUSAO_RECENTE: f64 = 30.0;
/// Animação depois de uma troca de estado; depois disso a abelha fica parada.
const ANIMACAO: f64 = 2.5;

pub struct Conclusao {
    pub id: i64,
    pub projeto_id: i64,
    pub em: f64,
}

pub struct Abelha {
    pub conclusoes: Vec<Conclusao>,
    pub resumo_aberto: bool,
    estado: Estado,
    mudou_em: f64,
}

/// O estado de um conjunto de tarefas, sem contar a comemoração (que é um momento).
pub fn estado_base<'a>(tarefas: impl Iterator<Item = &'a Tarefa>, agentes_rodando: bool) -> Estado {
    let (mut erro, mut aguardando, mut trabalhando) = (false, false, false);
    for t in tarefas {
        erro |= t.erro.is_some() && !t.erro_visto;
        aguardando |= t.coluna == Coluna::AguardandoVoce;
        trabalhando |= agentes_rodando && t.coluna == Coluna::Trabalhando && !t.agentes.is_empty();
    }
    if erro {
        Estado::Bugado
    } else if aguardando {
        Estado::Aguardando
    } else if trabalhando {
        Estado::Trabalhando
    } else {
        Estado::Dormindo
    }
}

/// Cor do ponto ao lado de um projeto na barra lateral; nenhuma quando está tudo quieto.
/// Um erro já visto sai da abelha, mas o ponto fica vermelho até ele ser resolvido:
/// a abelha chama atenção, o ponto mostra onde está o problema.
pub fn cor_ponto(estado: Estado, tem_erro: bool, concluiu_ha_pouco: bool) -> Option<Color32> {
    match estado {
        _ if tem_erro => Some(cores().erro),
        Estado::Aguardando => Some(cores().alerta),
        _ if concluiu_ha_pouco => Some(cores().destaque),
        Estado::Trabalhando => Some(cores().ok),
        _ => None,
    }
}

impl Abelha {
    pub fn new() -> Self {
        Self { conclusoes: Vec::new(), resumo_aberto: false, estado: Estado::Dormindo, mudou_em: 0.0 }
    }

    pub fn concluiu(&mut self, id: i64, projeto_id: i64, agora: f64) {
        self.conclusoes.push(Conclusao { id, projeto_id, em: agora });
    }

    /// Um erro novo interrompe a comemoração: ele é mais urgente.
    pub fn interromper_comemoracao(&mut self) {
        for c in &mut self.conclusoes {
            c.em = c.em.min(-COMEMORACAO);
        }
    }

    /// Comemora se alguma tarefa do escopo foi concluída há pouco. Várias
    /// conclusões seguidas contam a partir da última: viram uma comemoração só.
    pub fn atualizar(&mut self, base: Estado, no_escopo: impl Fn(i64) -> bool, agora: f64) -> Estado {
        let comemorando = self.conclusoes.iter().any(|c| agora - c.em < COMEMORACAO && no_escopo(c.projeto_id));
        let novo = if comemorando { Estado::Comemorando } else { base };
        if novo != self.estado {
            self.estado = novo;
            self.mudou_em = agora;
        }
        novo
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, agora: f64, linha: &str) -> egui::Response {
        let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 132.0), Sense::click());
        let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);

        // Anima só na troca de estado, na comemoração e em rajadas curtas
        // (a falha do bugado, uma piscada nos outros). No resto do tempo o
        // desenho usa um tempo parado e nenhum redesenho é pedido.
        let desde = agora - self.mudou_em;
        let ctx = ui.ctx().clone();
        let (intervalo, rajada) = if self.estado == Estado::Bugado { (4.0, 0.5) } else { (6.0, 0.35) };
        let t_visual = if desde < ANIMACAO || self.estado == Estado::Comemorando {
            ctx.request_repaint();
            agora
        } else {
            let fase = (desde - ANIMACAO) % intervalo;
            if fase < rajada {
                ctx.request_repaint();
                agora
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(intervalo - fase));
                self.tempo_parado()
            }
        };

        let pintor = ui.painter_at(rect);
        if resposta.hovered() || self.resumo_aberto {
            pintor.rect_filled(rect, 10.0, cores().realce);
        }
        let tempo = Tempo { total: t_visual as f32, no_estado: (t_visual - self.mudou_em) as f32 };
        mascote::desenhar(&pintor, rect.center() - vec2(0.0, 16.0), 78.0, self.estado, tempo);
        pintor.text(rect.center_bottom() - vec2(0.0, 26.0), egui::Align2::CENTER_CENTER, self.estado.nome(), egui::FontId::proportional(13.0), cores().texto);
        pintor.text(rect.center_bottom() - vec2(0.0, 10.0), egui::Align2::CENTER_CENTER, linha, egui::FontId::proportional(11.0), cores().suave);
        resposta
    }

    /// Um instante fixo depois da animação. No bugado, um instante sem rajada forte.
    fn tempo_parado(&self) -> f64 {
        let mut t = self.mudou_em + ANIMACAO;
        if self.estado == Estado::Bugado {
            let resto = t % 0.9;
            if resto < 0.3 {
                t += 0.35 - resto;
            }
        }
        t
    }
}

/// Resumo aberto ao clicar na abelha: uma linha por ocorrência, que leva ao cartão.
/// Retorna a tarefa clicada e a resposta da área, para fechar ao clicar fora.
pub fn resumo(
    ctx: &egui::Context,
    ancora: Pos2,
    tarefas: &[Tarefa],
    no_escopo: impl Fn(i64) -> bool,
    conclusoes: &[Conclusao],
    agentes_rodando: bool,
    agora: f64,
) -> (Option<i64>, egui::Response) {
    let mut escolhida = None;
    let area = egui::Area::new(Id::new("resumo-abelha")).order(Order::Foreground).pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(ancora).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(cores().superficie_alta).corner_radius(crate::tema::RAIO_SUPERFICIE).inner_margin(14).show(ui, |ui| {
            ui.set_width(360.0);
            ui.label(crate::tema::texto_forte("O que está acontecendo", 14.0).color(cores().texto));
            ui.add_space(6.0);
            let mut vazio = true;
            let mut linha = |ui: &mut egui::Ui, cor: Color32, titulo: String, detalhe: String, id: i64| {
                vazio = false;
                let resposta = ui
                    .vertical(|ui| {
                        ui.horizontal(|ui| {
                            let (ponto, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(ponto.center(), 3.5, cor);
                            ui.label(RichText::new(titulo).color(cores().texto));
                        });
                        ui.label(RichText::new(detalhe).color(cores().suave).size(11.5));
                    })
                    .response
                    .interact(Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resposta.clicked() {
                    escolhida = Some(id);
                }
                ui.add_space(4.0);
            };

            let visiveis = || tarefas.iter().filter(|t| no_escopo(t.projeto_id));
            for t in visiveis().filter(|t| t.erro.is_some() && !t.erro_visto) {
                linha(ui, cores().erro, format!("Erro em {}", t.projeto), format!("{}: {}", t.titulo, t.erro.unwrap_or_default()), t.id);
            }
            for t in visiveis().filter(|t| t.coluna == Coluna::AguardandoVoce) {
                linha(ui, cores().alerta, format!("Aguardando você em {}", t.projeto), format!("{}: {}", t.titulo, t.motivo.unwrap_or("pede resposta")), t.id);
            }
            for c in conclusoes.iter().rev().filter(|c| agora - c.em < CONCLUSAO_RECENTE && no_escopo(c.projeto_id)).take(5) {
                if let Some(t) = tarefas.iter().find(|t| t.id == c.id) {
                    linha(ui, cores().destaque, format!("Concluída em {}", t.projeto), format!("{}, há {:.0} s", t.titulo, agora - c.em), t.id);
                }
            }
            let rodando: Vec<&Tarefa> = visiveis().filter(|t| agentes_rodando && t.coluna == Coluna::Trabalhando && !t.agentes.is_empty()).collect();
            if let Some(primeira) = rodando.first() {
                let agentes: usize = rodando.iter().map(|t| t.agentes.len()).sum();
                linha(ui, cores().ok, format!("{} tarefas com agentes trabalhando", rodando.len()), format!("{agentes} agentes no total"), primeira.id);
            }
            if vazio {
                ui.label(RichText::new("Nada precisa de você agora.").color(cores().suave));
            }
        });
    });
    (escolhida, area.response)
}
