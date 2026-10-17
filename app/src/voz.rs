//! O ditado por voz na caixa de mensagem: a tela abre o `colmeia-voz` (um
//! programa à parte, com o microfone e o Whisper) só quando você aperta o
//! microfone, fala com ele por uma linha de JSON na entrada e na saída e
//! deixa que ele saia sozinho depois de um tempo parado. A tela nunca toca no
//! áudio: recebe só o texto, que entra na caixa para revisar.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use canal::voz::{Arquivos, Marcada, Processo, Resposta};
use eframe::egui;

/// O atalho de segurar para falar: Ctrl+Shift+M.
pub const ATALHO: &str = "Ctrl+Shift+M";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estado {
    Parado,
    /// O microfone foi pedido e o programa está abrindo.
    Abrindo,
    Ouvindo,
    Transcrevendo,
}

impl Estado {
    /// O microfone está (ou vai estar) aberto.
    pub fn ouvindo(self) -> bool {
        matches!(self, Estado::Abrindo | Estado::Ouvindo)
    }
}

/// Tira da fila (antes do terminal) o Ctrl+Shift+M apertado e, com o
/// microfone aberto pelo atalho, a tecla M solta. Devolve (apertou, soltou);
/// a repetição de tecla segurada não conta como apertar de novo.
pub fn tirar_atalho(eventos: &mut Vec<egui::Event>, segurado: bool) -> (bool, bool) {
    let (mut apertou, mut soltou) = (false, false);
    eventos.retain(|e| match e {
        egui::Event::Key { key: egui::Key::M, pressed: true, repeat, modifiers, .. } if modifiers.ctrl && modifiers.shift => {
            apertou |= !*repeat;
            false
        }
        egui::Event::Key { key: egui::Key::M, pressed: false, .. } if segurado => {
            soltou = true;
            false
        }
        _ => true,
    });
    (apertou, soltou)
}

/// O que a tela faz com uma resposta.
#[derive(Debug, PartialEq)]
pub enum Saida {
    Texto(String),
    Erro(String),
}

pub struct Ditado {
    processo: Option<Processo>,
    estado: Estado,
    /// O que chega das threads de leitura, marcado com o processo que mandou.
    respostas: (Sender<Marcada>, Receiver<Marcada>),
    geracao: u64,
    /// Os arquivos do modelo, perguntados ao núcleo na primeira vez.
    arquivos: Option<Arquivos>,
    /// A tarefa em que o microfone abriu: saindo dela, ele fecha.
    pub tarefa: i64,
    /// O microfone foi aberto pelo atalho (soltar a tecla para).
    pub pelo_atalho: bool,
}

impl Default for Ditado {
    fn default() -> Self {
        Ditado { processo: None, estado: Estado::Parado, respostas: mpsc::channel(), geracao: 0, arquivos: None, tarefa: 0, pelo_atalho: false }
    }
}

impl Ditado {
    pub fn estado(&self) -> Estado {
        self.estado
    }

    /// Abre o microfone (e o programa, se ainda não está aberto).
    pub fn ouvir(&mut self, ctx: &egui::Context, tarefa: i64, pelo_atalho: bool) -> Result<(), String> {
        if self.estado != Estado::Parado {
            return Ok(());
        }
        if self.processo.is_none() {
            self.abrir(ctx)?;
        }
        self.mandar("ouvir")?;
        self.estado = Estado::Abrindo;
        self.tarefa = tarefa;
        self.pelo_atalho = pelo_atalho;
        Ok(())
    }

    /// Fecha o microfone e pede a transcrição. Os pedidos vão em ordem: se o
    /// microfone ainda está abrindo, o programa abre, ouve o pouco que deu e para.
    pub fn parar(&mut self) {
        if self.estado.ouvindo() {
            self.estado = if self.mandar("parar").is_ok() { Estado::Transcrevendo } else { Estado::Parado };
        }
    }

    /// Fecha o microfone e joga o áudio fora.
    pub fn cancelar(&mut self) {
        if self.estado.ouvindo() {
            let _ = self.mandar("cancelar");
            self.estado = Estado::Parado;
        }
    }

    fn mandar(&mut self, comando: &str) -> Result<(), String> {
        let Some(p) = self.processo.as_mut() else { return Err("O ditado não está aberto.".into()) };
        if p.mandar(&canal::voz::pedido(comando)).is_err() {
            self.fechar();
            return Err("O ditado fechou sem responder. Tente de novo.".into());
        }
        Ok(())
    }

    fn abrir(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let arquivos = match &self.arquivos {
            Some(a) => a.clone(),
            None => {
                let a = Arquivos::do_nucleo()?;
                self.arquivos = Some(a.clone());
                a
            }
        };
        self.geracao += 1;
        let ctx = ctx.clone();
        self.processo = Some(Processo::abrir(&arquivos, self.geracao, self.respostas.0.clone(), Arc::new(move || ctx.request_repaint()))?);
        Ok(())
    }

    /// Fecha a entrada (o programa sai) e recolhe o processo.
    fn fechar(&mut self) {
        self.processo = None;
        self.estado = Estado::Parado;
    }

    /// As respostas que chegaram, aplicadas ao estado.
    pub fn receber(&mut self) -> Vec<Saida> {
        let mut saidas = Vec::new();
        while let Ok((geracao, r)) = self.respostas.1.try_recv() {
            if self.processo.as_ref().is_none_or(|p| p.geracao != geracao) {
                continue;
            }
            if let Some(s) = self.aplicar(r) {
                saidas.push(s);
            }
        }
        saidas
    }

    fn aplicar(&mut self, r: Resposta) -> Option<Saida> {
        match r {
            Resposta::Iniciado
            | Resposta::Vigiando { .. }
            | Resposta::Chamado
            | Resposta::Falando
            | Resposta::Falou
            | Resposta::Amostra { .. }
            | Resposta::Aprendido => None,
            Resposta::Ouvindo => {
                // Um "parar" já pode ter saído: o estado só avança.
                if self.estado == Estado::Abrindo {
                    self.estado = Estado::Ouvindo;
                }
                None
            }
            Resposta::Transcrevendo { .. } => {
                self.estado = Estado::Transcrevendo;
                None
            }
            Resposta::Texto { texto } => {
                self.estado = Estado::Parado;
                if texto.trim().is_empty() {
                    Some(Saida::Erro("Não entendi o que foi dito. Tente de novo, mais perto do microfone.".into()))
                } else {
                    Some(Saida::Texto(texto))
                }
            }
            Resposta::Erro { mensagem } => {
                self.estado = Estado::Parado;
                Some(Saida::Erro(mensagem))
            }
            // Parado por muito tempo: sai sozinho e leva o modelo junto.
            Resposta::Saindo { .. } => {
                self.fechar();
                None
            }
            Resposta::Fim => {
                let no_meio = self.estado != Estado::Parado;
                self.fechar();
                no_meio.then(|| Saida::Erro("O ditado fechou sem terminar. Tente de novo.".into()))
            }
        }
    }

    /// Desligar o ditado: fecha o programa na hora (sem esperar o tempo parado).
    pub fn desligar(&mut self) {
        self.cancelar();
        self.fechar();
        self.arquivos = None;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tecla(pressed: bool, repeat: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key: egui::Key::M, physical_key: None, pressed, repeat, modifiers }
    }

    #[test]
    fn segurar_o_atalho_abre_e_soltar_fecha() {
        let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let mut eventos = vec![tecla(true, false, ctrl_shift), tecla(true, true, ctrl_shift), egui::Event::Text("x".into())];
        assert_eq!(tirar_atalho(&mut eventos, false), (true, false));
        // O terminal não vê o atalho, só o resto.
        assert_eq!(eventos, vec![egui::Event::Text("x".into())]);
        // Segurando, a repetição não abre de novo; soltar (com ou sem Ctrl) fecha.
        let mut eventos = vec![tecla(true, true, ctrl_shift), tecla(false, false, egui::Modifiers::NONE)];
        assert_eq!(tirar_atalho(&mut eventos, true), (false, true));
        assert!(eventos.is_empty());
        // Sem o ditado pelo atalho, o M solto e o M sem Ctrl+Shift seguem para quem digita.
        let mut eventos = vec![tecla(false, false, egui::Modifiers::NONE), tecla(true, false, egui::Modifiers::SHIFT)];
        assert_eq!(tirar_atalho(&mut eventos, false), (false, false));
        assert_eq!(eventos.len(), 2);
    }

    #[test]
    fn o_texto_volta_para_a_caixa_e_o_estado_para() {
        let mut d = Ditado { estado: Estado::Abrindo, ..Default::default() };
        assert_eq!(d.aplicar(Resposta::Ouvindo), None);
        assert_eq!(d.estado(), Estado::Ouvindo);
        assert_eq!(d.aplicar(Resposta::Transcrevendo { segundos: 1.0 }), None);
        assert_eq!(d.estado(), Estado::Transcrevendo);
        assert_eq!(d.aplicar(Resposta::Texto { texto: "Abra a tarefa.".into() }), Some(Saida::Texto("Abra a tarefa.".into())));
        assert_eq!(d.estado(), Estado::Parado);
    }

    #[test]
    fn texto_vazio_e_erro_viram_aviso() {
        let mut d = Ditado { estado: Estado::Transcrevendo, ..Default::default() };
        assert!(matches!(d.aplicar(Resposta::Texto { texto: "  ".into() }), Some(Saida::Erro(_))));
        d.estado = Estado::Ouvindo;
        assert_eq!(d.aplicar(Resposta::Erro { mensagem: "Não achei um microfone.".into() }), Some(Saida::Erro("Não achei um microfone.".into())));
        assert_eq!(d.estado(), Estado::Parado);
    }

    #[test]
    fn o_programa_saindo_no_meio_avisa_e_parado_nao() {
        let mut d = Ditado { estado: Estado::Ouvindo, ..Default::default() };
        assert!(matches!(d.aplicar(Resposta::Fim), Some(Saida::Erro(_))));
        assert_eq!(d.estado(), Estado::Parado);
        assert_eq!(d.aplicar(Resposta::Saindo { motivo: "parado".into() }), None);
        assert_eq!(d.aplicar(Resposta::Fim), None);
        assert!(d.processo.is_none());
    }

    #[test]
    fn ouvindo_que_chega_depois_do_parar_nao_volta_o_estado() {
        let mut d = Ditado { estado: Estado::Transcrevendo, ..Default::default() };
        d.aplicar(Resposta::Ouvindo);
        assert_eq!(d.estado(), Estado::Transcrevendo);
    }
}
