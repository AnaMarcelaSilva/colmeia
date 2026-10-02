//! A abelha da barra lateral: resume o que mais precisa de você no escopo que
//! está na tela (o perfil inteiro ou um projeto) e só anima quando o estado
//! muda ou o mouse chega nela.

use std::time::Duration;

use eframe::egui::{self, Color32, Id, Order, Pos2, RichText, Sense, vec2};
use mascote::{Estado, Tempo};

use crate::dados::{Coluna, Tarefa};
use crate::tema::{EstadoVisual, cores};

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
    /// Até quando a abelha anima (troca de estado ou mouse em cima).
    animar_ate: f64,
    em_cima: bool,
}

/// Quadros por segundo da animação: suave o bastante para uma abelha de 78 px
/// e metade do custo de redesenhar a janela inteira a 60 por segundo.
const QUADROS_ANIMACAO: f64 = 30.0;

/// O estado de um conjunto de tarefas, sem contar a comemoração (que é um momento).
/// Prioridade: erro, aguardando você, trabalhando.
pub fn estado_base<'a>(tarefas: impl Iterator<Item = &'a Tarefa>, agentes_rodando: bool) -> Estado {
    let (mut erro, mut aguardando, mut trabalhando) = (false, false, false);
    for t in tarefas {
        erro |= t.erro.is_some() && !t.erro_visto;
        aguardando |= t.coluna == Coluna::AguardandoVoce || t.agentes.iter().any(|a| matches!(a.visual(), EstadoVisual::PedeAprovacao | EstadoVisual::SuaVez));
        trabalhando |= agentes_rodando && t.agentes.iter().any(|a| a.ativo && a.visual() == EstadoVisual::Trabalhando);
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
        Self { conclusoes: Vec::new(), resumo_aberto: false, estado: Estado::Dormindo, mudou_em: 0.0, animar_ate: 0.0, em_cima: false }
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
            self.animar_ate = agora + ANIMACAO;
        }
        novo
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, agora: f64, linha: &str) -> egui::Response {
        let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 132.0), Sense::click());
        let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);

        // Anima só depois de uma troca de estado, durante a comemoração e por
        // uns segundos quando o mouse chega nela; no resto do tempo fica parada
        // e não pede redesenho nenhum (a janela inteira é redesenhada a cada
        // quadro, então uma abelha sempre animada custaria a janela inteira).
        let ctx = ui.ctx().clone();
        let em_cima = resposta.hovered();
        if em_cima && !self.em_cima {
            self.animar_ate = self.animar_ate.max(agora + ANIMACAO);
        }
        self.em_cima = em_cima;
        let animando = agora < self.animar_ate || self.estado == Estado::Comemorando;
        let t_visual = if animando {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / QUADROS_ANIMACAO));
            agora
        } else {
            self.tempo_parado()
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

/// A linha embaixo da abelha: quantos precisam de você, ou o que está acontecendo.
pub fn linha_de_estado<'a>(estado: Estado, tarefas: impl Iterator<Item = &'a Tarefa> + Clone) -> String {
    let plural = |n: usize, um: &str, varios: &str| if n == 1 { format!("1 {um}") } else { format!("{n} {varios}") };
    let agentes = || tarefas.clone().flat_map(|t| &t.agentes);
    match estado {
        Estado::Bugado => {
            let n = agentes().filter(|a| a.erro_novo()).count() + tarefas.clone().filter(|t| t.erro.is_some() && !t.erro_visto && t.agentes.is_empty()).count();
            plural(n.max(1), "erro", "erros")
        }
        Estado::Aguardando => {
            let n = agentes().filter(|a| matches!(a.visual(), EstadoVisual::PedeAprovacao | EstadoVisual::SuaVez)).count();
            let n = n.max(tarefas.clone().filter(|t| t.coluna == Coluna::AguardandoVoce).count());
            plural(n, "esperando você", "esperando você")
        }
        Estado::Comemorando => "tarefa concluída".to_string(),
        Estado::Trabalhando => {
            plural(agentes().filter(|a| a.ativo && a.visual() == EstadoVisual::Trabalhando).count(), "agente trabalhando", "agentes trabalhando")
        }
        Estado::Dormindo => "tudo quieto".to_string(),
    }
}

/// Resumo aberto ao clicar na abelha: uma linha por agente que precisa de
/// você (e por tarefa concluída), que leva à tarefa com foco nesse agente.
/// Retorna a tarefa (e o agente) clicados e a resposta da área, para fechar ao clicar fora.
pub fn resumo(
    ctx: &egui::Context,
    ancora: Pos2,
    tarefas: &[Tarefa],
    no_escopo: impl Fn(i64) -> bool,
    conclusoes: &[Conclusao],
    agentes_rodando: bool,
    agora: f64,
) -> (Option<(i64, Option<i64>)>, egui::Response) {
    let mut escolhida = None;
    let area = egui::Area::new(Id::new("resumo-abelha")).order(Order::Foreground).pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(ancora).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(cores().superficie_alta).corner_radius(crate::tema::RAIO_SUPERFICIE).inner_margin(14).show(ui, |ui| {
            ui.set_width(360.0);
            ui.label(crate::tema::texto_forte("O que está acontecendo", 14.0).color(cores().texto));
            ui.add_space(6.0);
            let mut vazio = true;
            // Cada texto vem em duas partes: o começo pode ser cortado com "…",
            // o fim (o projeto, a hora) fica sempre inteiro.
            let mut linha = |ui: &mut egui::Ui, estado: EstadoVisual, titulo: (String, String), detalhe: (String, String), alvo: (i64, Option<i64>)| {
                vazio = false;
                let (rect, resposta) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
                let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resposta.hovered() {
                    ui.painter().rect_filled(rect, crate::tema::RAIO_CONTROLE, cores().realce);
                }
                let interno = rect.shrink2(vec2(6.0, 4.0));
                crate::tema::ponto(ui.painter(), egui::pos2(interno.left() + 4.0, interno.top() + 9.0), 3.5, estado);
                let largura = interno.width() - 16.0;
                let (fonte, fonte_detalhe) = (egui::FontId::proportional(13.5), egui::FontId::proportional(11.5));
                let altura = ui.fonts_mut(|f| f.row_height(&fonte));
                let altura_detalhe = ui.fonts_mut(|f| f.row_height(&fonte_detalhe));
                let x = interno.left() + 16.0;
                let (texto, suave) = (cores().texto, cores().suave);
                let cortou_titulo = crate::tema::texto_com_fim(
                    ui.painter(),
                    egui::pos2(x, interno.top() + 9.0 - altura / 2.0),
                    &titulo.0,
                    &titulo.1,
                    fonte,
                    texto,
                    texto,
                    largura,
                );
                let cortou_detalhe = crate::tema::texto_com_fim(
                    ui.painter(),
                    egui::pos2(x, interno.bottom() - 7.0 - altura_detalhe / 2.0),
                    &detalhe.0,
                    &detalhe.1,
                    fonte_detalhe,
                    suave,
                    suave,
                    largura,
                );
                // A dica com o texto inteiro só quando algo foi cortado.
                let resposta = if cortou_titulo || cortou_detalhe {
                    resposta.on_hover_text(format!("{}{}\n{}{}", titulo.0, titulo.1, detalhe.0, detalhe.1))
                } else {
                    resposta
                };
                if resposta.clicked() {
                    escolhida = Some(alvo);
                }
            };

            let visiveis = || tarefas.iter().filter(|t| no_escopo(t.projeto_id));
            // Erros que você ainda não viu, um por agente.
            for t in visiveis() {
                for a in t.agentes.iter().filter(|a| a.erro_novo()) {
                    let hora = a.fim.as_ref().map(|f| format!(" · às {}", f.hora)).unwrap_or_default();
                    linha(
                        ui,
                        EstadoVisual::Erro,
                        (format!("{} parou com erro", a.nome_com_papel()), format!(" em {}", t.projeto)),
                        (format!("“{}”", t.titulo), hora),
                        (t.id, Some(a.id)),
                    );
                }
                // Na demonstração o erro é da tarefa, sem agente.
                if t.agentes.iter().all(|a| !a.erro_novo()) && t.erro.is_some() && !t.erro_visto && t.agentes.iter().all(|a| a.fim.is_none()) {
                    linha(
                        ui,
                        EstadoVisual::Erro,
                        ("Erro".into(), format!(" em {}", t.projeto)),
                        (format!("{}: {}", t.titulo, t.erro.clone().unwrap_or_default()), String::new()),
                        (t.id, None),
                    );
                }
            }
            // Quem espera você, um por agente; cartões em "Aguardando" sem agente esperando também.
            for t in visiveis() {
                let mut algum = false;
                for a in &t.agentes {
                    let estado = a.visual();
                    if matches!(estado, EstadoVisual::PedeAprovacao | EstadoVisual::SuaVez) {
                        algum = true;
                        let acao = if estado == EstadoVisual::PedeAprovacao { "pede aprovação" } else { "espera sua resposta" };
                        let desde = if a.desde.is_empty() { String::new() } else { format!(" · desde {}", a.desde) };
                        linha(
                            ui,
                            estado,
                            (format!("{} {acao}", a.nome_com_papel()), format!(" em {}", t.projeto)),
                            (format!("“{}”", t.titulo), desde),
                            (t.id, Some(a.id)),
                        );
                    }
                }
                if !algum && t.coluna == Coluna::AguardandoVoce {
                    let detalhe = match &t.motivo {
                        Some(m) => format!("“{}”: {m}", t.titulo),
                        None => format!("“{}”", t.titulo),
                    };
                    linha(ui, EstadoVisual::SuaVez, ("Aguardando você".into(), format!(" em {}", t.projeto)), (detalhe, String::new()), (t.id, None));
                }
            }
            for c in conclusoes.iter().rev().filter(|c| agora - c.em < CONCLUSAO_RECENTE && no_escopo(c.projeto_id)).take(5) {
                if let Some(t) = tarefas.iter().find(|t| t.id == c.id) {
                    linha(
                        ui,
                        EstadoVisual::Concluiu,
                        ("Concluída".into(), format!(" em {}", t.projeto)),
                        (t.titulo.clone(), format!(", há {:.0} s", agora - c.em)),
                        (t.id, None),
                    );
                }
            }
            let rodando: Vec<&Tarefa> =
                visiveis().filter(|t| agentes_rodando && t.agentes.iter().any(|a| a.ativo && a.visual() == EstadoVisual::Trabalhando)).collect();
            if let Some(primeira) = rodando.first() {
                // Conta só os agentes que estão trabalhando, não os parados da mesma tarefa.
                let agentes: usize = rodando.iter().map(|t| t.agentes.iter().filter(|a| a.ativo && a.visual() == EstadoVisual::Trabalhando).count()).sum();
                let tarefas_texto = if rodando.len() == 1 {
                    "1 tarefa com agentes trabalhando".to_string()
                } else {
                    format!("{} tarefas com agentes trabalhando", rodando.len())
                };
                let agentes_texto = if agentes == 1 { "1 agente no total".to_string() } else { format!("{agentes} agentes no total") };
                linha(ui, EstadoVisual::Trabalhando, (tarefas_texto, String::new()), (agentes_texto, String::new()), (primeira.id, None));
            }
            if vazio {
                ui.label(RichText::new("Nada precisa de você agora.").color(cores().suave));
            }
        });
    });
    (escolhida, area.response)
}
