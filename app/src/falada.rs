//! Os comandos falados na janela: clicar na abelha da barra lateral (ou o
//! atalho Ctrl+Alt+Espaço) chama a abelha, que mostra num balão (e fala, com
//! a voz dela ligada) a lista numerada do que importa agora; em seguida o
//! microfone abre pelo `colmeia-voz` para o comando ("abrir a um"), a frase
//! vai ao núcleo, que entende e responde. O que pede "sim" mostra Sim e Não
//! e abre o microfone para o "sim" falado. Com a pergunta livre com IA ligada, a frase também pode ser
//! escrita no resumo da abelha (sem microfone), e o que não é comando vai ao
//! Claude Code pelo núcleo. Tudo desligado por padrão: sem os comandos e sem
//! a pergunta livre, nada disto abre.
//!
//! A tela não redesenha sozinha: o balão pede um quadro só para sumir.

use std::sync::Arc;

use canal::voz::{Conversa, Fase, Saida};
use eframe::egui::{self, Id, Order, Pos2, RichText, vec2};

use crate::api;
use crate::tema::{self, cores};

/// O atalho que abre o microfone para um comando (na abelha fixa, vale fora da janela).
pub const ATALHO: &str = "Ctrl+Alt+Espaço";

/// Quanto tempo a resposta fica no balão (com uma pergunta, até a resposta).
const RESPOSTA_SEGUNDOS: f64 = 10.0;
const PERGUNTA_SEGUNDOS: f64 = 60.0;

/// O que o balão mostra.
#[derive(Clone, Debug, PartialEq)]
pub struct Balao {
    pub texto: String,
    /// O que foi ouvido (embaixo, pequeno).
    pub ouvido: String,
    /// A frase foi escrita, não falada.
    pub escrito: bool,
    pub erro: bool,
    pub abrir: Option<i64>,
    /// A resposta é a própria lista (o rodapé ensina o que dizer).
    pub de_lista: bool,
    pub ate: f64,
}

/// O que a janela faz depois de um quadro do balão.
#[derive(Debug, PartialEq)]
pub enum Pedido {
    AbrirTarefa(i64),
    /// "Responder a um": a tarefa aberta no agente, com o ditado ligado na caixa dele.
    Responder {
        tarefa: i64,
        agente: i64,
    },
}

#[derive(Default)]
pub struct Falada {
    conversa: Option<Conversa>,
    balao: Option<Balao>,
    /// Os comandos falados (o microfone); sem eles, só a frase escrita.
    comandos: bool,
    /// A frase sendo escrita no resumo da abelha.
    pub rascunho: String,
}

impl Falada {
    /// Liga ou desliga conforme o perfil (os comandos, a pergunta livre, a
    /// voz da abelha e o efeito).
    pub fn ajustar(&mut self, perfil: Option<&api::Perfil>, ctx: &egui::Context) {
        let Some(p) = perfil.filter(|p| p.voz.voz_comandos || p.voz.voz_ia) else {
            self.desligar();
            return;
        };
        if self.conversa.as_ref().is_some_and(|c| c.perfil != p.id) {
            self.desligar();
        }
        let ctx = ctx.clone();
        let c = self.conversa.get_or_insert_with(|| Conversa::nova(p.id, Arc::new(move || ctx.request_repaint())));
        c.falar = p.voz.voz_abelha;
        c.efeito = p.voz.voz_efeito;
        if !p.voz.voz_comandos {
            c.cancelar();
        }
        self.comandos = p.voz.voz_comandos;
    }

    pub fn desligar(&mut self) {
        if let Some(c) = self.conversa.as_mut() {
            c.fechar();
        }
        self.conversa = None;
        self.balao = None;
        self.comandos = false;
        self.rascunho.clear();
    }

    /// Dá para mandar uma frase (falada ou escrita) à abelha.
    pub fn ligada(&self) -> bool {
        self.conversa.is_some()
    }

    /// O clique na abelha abre o microfone (os comandos falados estão ligados).
    pub fn ouve(&self) -> bool {
        self.conversa.is_some() && self.comandos
    }

    /// Manda a frase escrita no resumo; a resposta chega no balão.
    pub fn escrever(&mut self, agora: f64) {
        let Some(c) = self.conversa.as_mut() else { return };
        let frase = self.rascunho.trim().to_string();
        if c.escrever(&frase) {
            self.balao =
                Some(Balao { texto: String::new(), ouvido: frase, escrito: true, erro: false, abrir: None, de_lista: false, ate: agora + PERGUNTA_SEGUNDOS });
            self.rascunho.clear();
        }
    }

    /// Ouvindo um comando (não só o nome).
    pub fn ouvindo_comando(&self) -> bool {
        self.conversa.as_ref().is_some_and(|c| matches!(c.fase(), Fase::Abrindo | Fase::Ouvindo))
    }

    pub fn microfone_aberto(&self) -> bool {
        self.conversa.as_ref().is_some_and(Conversa::microfone_aberto)
    }

    /// Clique na abelha (ou o atalho): chama a abelha, que fala a lista e
    /// abre o microfone; clicado de novo ouvindo, para e transcreve o que
    /// ouviu (o clique manda, sem esperar a pausa).
    pub fn alternar(&mut self) {
        let Some(c) = self.conversa.as_mut().filter(|_| self.comandos) else { return };
        if matches!(c.fase(), Fase::Abrindo | Fase::Ouvindo) {
            c.parar();
            return;
        }
        if c.ouvir_agora() {
            return;
        }
        c.encerrar();
        c.chamar();
        self.balao = None;
    }

    /// Tira da fila (antes do terminal) o Ctrl+Alt+Espaço.
    pub fn tirar_atalho(eventos: &mut Vec<egui::Event>) -> bool {
        let mut apertou = false;
        eventos.retain(|e| match e {
            egui::Event::Key { key: egui::Key::Space, pressed, modifiers, .. } if modifiers.ctrl && modifiers.alt => {
                apertou |= *pressed;
                false
            }
            _ => true,
        });
        apertou
    }

    /// O que chegou da voz e do núcleo; devolve o que a janela precisa fazer.
    pub fn receber(&mut self, agora: f64) -> Vec<Pedido> {
        let Some(c) = self.conversa.as_mut() else { return Vec::new() };
        let mut pedidos = Vec::new();
        for s in c.receber() {
            match s {
                Saida::Ouvido(texto) => {
                    self.balao = Some(Balao {
                        texto: String::new(),
                        ouvido: texto,
                        escrito: false,
                        erro: false,
                        abrir: None,
                        de_lista: false,
                        ate: agora + PERGUNTA_SEGUNDOS,
                    });
                }
                Saida::Resposta(r) => {
                    let (ouvido, escrito) = self.balao.as_ref().map(|b| (b.ouvido.clone(), b.escrito)).unwrap_or_default();
                    let espera = !r.confirmar.is_empty() || r.ouvir;
                    let abrir = (r.abrir > 0).then_some(r.abrir);
                    match abrir {
                        Some(tarefa) if r.ditar && r.agente > 0 => pedidos.push(Pedido::Responder { tarefa, agente: r.agente }),
                        Some(t) => pedidos.push(Pedido::AbrirTarefa(t)),
                        None => {}
                    }
                    // Uma resposta longa (a pergunta livre) fica o tempo de ler.
                    let tempo = if espera { PERGUNTA_SEGUNDOS } else { RESPOSTA_SEGUNDOS.max(r.texto.chars().count() as f64 / 15.0) };
                    let de_lista = r.lista.is_some();
                    self.balao = Some(Balao { texto: r.texto, ouvido, escrito, erro: false, abrir, de_lista, ate: agora + tempo });
                }
                Saida::Erro(e) => {
                    self.balao = Some(Balao {
                        texto: e,
                        ouvido: String::new(),
                        escrito: false,
                        erro: true,
                        abrir: None,
                        de_lista: false,
                        ate: agora + RESPOSTA_SEGUNDOS,
                    });
                }
                Saida::Chamado | Saida::Vigiando(_) | Saida::Falando(_) | Saida::Amostra(_) | Saida::Aprendido | Saida::Nota { .. } => {}
            }
        }
        pedidos
    }

    /// O texto de agora no balão: ouvindo, entendendo ou a resposta.
    fn estado(&self) -> Option<(String, bool)> {
        let c = self.conversa.as_ref()?;
        match c.fase() {
            Fase::Abrindo | Fase::Ouvindo if c.lista().is_some() => Some(("Ouvindo…".into(), false)),
            Fase::Abrindo | Fase::Ouvindo => Some(("Ouvindo… diga o comando.".into(), false)),
            Fase::Transcrevendo => Some(("Entendendo…".into(), false)),
            Fase::Pensando => Some(("Um instante…".into(), false)),
            Fase::Parada => None,
        }
    }

    /// O balão ao lado da abelha, com Sim e Não quando há pergunta. Some no
    /// tempo (sem redesenhar até lá) ou no x.
    pub fn balao(&mut self, ctx: &egui::Context, nome: &str, ancora: Pos2, agora: f64) {
        if self.balao.as_ref().is_some_and(|b| agora >= b.ate) {
            self.balao = None;
            if let Some(c) = self.conversa.as_mut() {
                c.pergunta = None;
            }
        }
        let andamento = self.estado();
        let Some(c) = self.conversa.as_mut() else { return };
        let (mut texto, erro, mut ouvido) = match (&andamento, &self.balao) {
            (Some((t, _)), b) => (t.clone(), false, b.as_ref().map(|b| b.ouvido.clone()).unwrap_or_default()),
            (None, Some(b)) => (b.texto.clone(), b.erro, b.ouvido.clone()),
            (None, None) => return,
        };
        // Com a lista aberta, ela fica no balão; embaixo, o que acontece agora
        // (ouvindo, a resposta curta) ou o que dá para dizer.
        let mut rodape = None;
        if let Some(l) = c.lista() {
            let agora_diz = match (&andamento, &self.balao) {
                (Some((t, _)), _) => format!("{t} {}", l.dica),
                (None, Some(b)) if !b.de_lista => b.texto.clone(),
                _ => l.dica.clone(),
            };
            texto = l.texto("Nada espera você agora.");
            rodape = Some(agora_diz);
            // O que foi ouvido fica só na resposta (para ver o que a abelha entendeu).
            if self.balao.as_ref().is_none_or(|b| b.de_lista) || andamento.is_some() {
                ouvido.clear();
            }
        }
        let escrito = self.balao.as_ref().is_some_and(|b| b.escrito);
        let pergunta = andamento.is_none() && c.pergunta.is_some();
        let mut fechar = false;
        let mut resposta = None;
        egui::Area::new(Id::new("balao-falado")).order(Order::Foreground).pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(ancora).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(cores().superficie_alta).corner_radius(tema::RAIO_SUPERFICIE).inner_margin(14).show(ui, |ui| {
                ui.set_width(320.0);
                ui.horizontal(|ui| {
                    if c.microfone_aberto() {
                        // O microfone aberto fica sempre à vista.
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 7.0, cores().erro);
                        tema::desenhar_icone(ui.painter(), r.center(), tema::Icone::Microfone, egui::Color32::WHITE);
                    }
                    ui.label(tema::texto_forte(nome, 13.5).color(cores().texto));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        fechar = tema::botao_icone(ui, tema::Icone::Fechar, 20.0).on_hover_text("Fechar").clicked();
                    });
                });
                ui.add_space(4.0);
                ui.label(RichText::new(&texto).size(13.5).color(if erro { cores().erro } else { cores().texto }));
                if let Some(r) = &rodape {
                    ui.add_space(6.0);
                    ui.label(RichText::new(r).size(11.5).color(cores().suave));
                }
                if !ouvido.is_empty() {
                    ui.add_space(4.0);
                    let quem = if escrito { "Você escreveu" } else { "Você disse" };
                    ui.label(RichText::new(format!("{quem}: \u{201c}{ouvido}\u{201d}")).size(11.5).color(cores().suave));
                }
                if pergunta {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if tema::botao_principal(ui, "Sim", true).clicked() {
                            resposta = Some(true);
                        }
                        if tema::botao_secundario(ui, "Não").clicked() {
                            resposta = Some(false);
                        }
                        ui.label(RichText::new("ou diga \u{201c}sim\u{201d}").size(11.5).color(cores().suave));
                    });
                }
            });
        });
        if let Some(sim) = resposta {
            c.responder(sim);
            self.balao = None;
        } else if fechar {
            c.encerrar();
            self.balao = None;
        }
        // Sem animação: um quadro só, na hora de sumir.
        if let Some(b) = &self.balao {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((b.ate - agora).max(0.05)));
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_atalho_sai_da_fila_antes_do_terminal() {
        let tecla = |pressed, modifiers| egui::Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers };
        let ctrl_alt = egui::Modifiers::CTRL | egui::Modifiers::ALT;
        let mut eventos = vec![tecla(true, ctrl_alt), tecla(false, ctrl_alt), egui::Event::Text(" ".into())];
        assert!(Falada::tirar_atalho(&mut eventos));
        assert_eq!(eventos, vec![egui::Event::Text(" ".into())]);
        // Espaço sozinho (ou com Ctrl) é do terminal.
        let mut eventos = vec![tecla(true, egui::Modifiers::CTRL), tecla(true, egui::Modifiers::NONE)];
        assert!(!Falada::tirar_atalho(&mut eventos));
        assert_eq!(eventos.len(), 2);
    }

    #[test]
    fn so_a_pergunta_livre_nao_abre_o_microfone() {
        let perfil: api::Perfil = serde_json::from_value(serde_json::json!({ "id": 1, "nome": "Pessoal", "tema": "escuro", "voz_ia": true })).unwrap();
        let mut f = Falada::default();
        f.ajustar(Some(&perfil), &egui::Context::default());
        assert!(f.ligada() && !f.ouve());
        f.alternar();
        assert!(!f.microfone_aberto());
        // Sem texto, nada vai ao núcleo.
        f.rascunho = "   ".into();
        f.escrever(0.0);
        assert!(f.balao.is_none());
        f.ajustar(None, &egui::Context::default());
        assert!(!f.ligada() && f.rascunho.is_empty());
    }

    #[test]
    fn desligada_nao_faz_nada() {
        let mut f = Falada::default();
        assert!(!f.ligada() && !f.microfone_aberto());
        f.alternar();
        assert!(f.receber(0.0).is_empty());
    }
}
