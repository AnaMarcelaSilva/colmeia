//! O ditado por voz na caixa de mensagem: a tela abre o `colmeia-voz` (um
//! programa à parte, com o microfone e o Whisper) só quando você aperta o
//! microfone, fala com ele por uma linha de JSON na entrada e na saída e
//! deixa que ele saia sozinho depois de um tempo parado. A tela nunca toca no
//! áudio: recebe só o texto, que entra na caixa para revisar.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
use serde::Deserialize;

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

/// O que o `colmeia-voz` responde (veja voz/src/protocolo.rs).
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "evento", rename_all = "snake_case")]
pub enum Resposta {
    Iniciado,
    Ouvindo,
    Transcrevendo {
        #[serde(default)]
        segundos: f32,
    },
    Texto {
        texto: String,
    },
    Erro {
        mensagem: String,
    },
    Saindo {
        #[serde(default)]
        motivo: String,
    },
    /// A saída do programa fechou (ele saiu).
    #[serde(skip)]
    Fim,
}

pub fn ler_resposta(linha: &str) -> Option<Resposta> {
    serde_json::from_str(linha.trim()).ok()
}

/// O que a tela faz com uma resposta.
#[derive(Debug, PartialEq)]
pub enum Saida {
    Texto(String),
    Erro(String),
}

/// Uma resposta com o número do processo que a mandou.
type Marcada = (u64, Resposta);

/// Fechar a Colmeia fecha a entrada do programa, e ele sai sozinho.
struct Processo {
    filho: Child,
    entrada: ChildStdin,
    geracao: u64,
}

pub struct Ditado {
    processo: Option<Processo>,
    estado: Estado,
    /// O que chega das threads de leitura, marcado com o processo que mandou.
    respostas: (Sender<Marcada>, Receiver<Marcada>),
    geracao: u64,
    /// O arquivo do modelo, perguntado ao núcleo na primeira vez.
    modelo: Option<String>,
    /// A tarefa em que o microfone abriu: saindo dela, ele fecha.
    pub tarefa: i64,
    /// O microfone foi aberto pelo atalho (soltar a tecla para).
    pub pelo_atalho: bool,
}

impl Default for Ditado {
    fn default() -> Self {
        Ditado { processo: None, estado: Estado::Parado, respostas: mpsc::channel(), geracao: 0, modelo: None, tarefa: 0, pelo_atalho: false }
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
        let linha = format!("{{\"comando\":\"{comando}\"}}\n");
        if p.entrada.write_all(linha.as_bytes()).and_then(|_| p.entrada.flush()).is_err() {
            self.fechar();
            return Err("O ditado fechou sem responder. Tente de novo.".into());
        }
        Ok(())
    }

    fn abrir(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let modelo = match &self.modelo {
            Some(m) => m.clone(),
            None => {
                let estado = crate::api::estado_voz().map_err(|e| format!("Não consegui falar com o núcleo: {e}"))?;
                if estado.estado != "pronto" || estado.caminho.is_empty() {
                    return Err("O modelo de voz não está baixado. Ligue o ditado em Ajustes.".into());
                }
                self.modelo = Some(estado.caminho.clone());
                estado.caminho
            }
        };
        let programa = canal::executavel("colmeia-voz", "COLMEIA_VOZ").ok_or("Não encontrei o programa colmeia-voz ao lado da Colmeia.")?;
        let mut comando = Command::new(programa);
        comando.arg("--modelo").arg(&modelo).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        // No Windows, sem abrir uma janela de console.
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut comando, 0x0800_0000);
        let mut filho = comando.spawn().map_err(|e| format!("Não consegui abrir o ditado: {e}"))?;
        let (Some(entrada), Some(saida)) = (filho.stdin.take(), filho.stdout.take()) else {
            let _ = filho.kill();
            let _ = filho.wait();
            return Err("Não consegui abrir o ditado.".into());
        };
        self.geracao += 1;
        let (geracao, envio, ctx) = (self.geracao, self.respostas.0.clone(), ctx.clone());
        std::thread::spawn(move || {
            for linha in BufReader::new(saida).lines() {
                let Ok(linha) = linha else { break };
                if let Some(r) = ler_resposta(&linha) {
                    let _ = envio.send((geracao, r));
                    ctx.request_repaint();
                }
            }
            let _ = envio.send((geracao, Resposta::Fim));
            ctx.request_repaint();
        });
        self.processo = Some(Processo { filho, entrada, geracao });
        Ok(())
    }

    /// Fecha a entrada (o programa sai) e recolhe o processo.
    fn fechar(&mut self) {
        if let Some(Processo { mut filho, entrada, .. }) = self.processo.take() {
            drop(entrada);
            // Já saiu ou sai em seguida: recolhe numa thread para não travar a tela.
            std::thread::spawn(move || {
                let _ = filho.wait();
            });
        }
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
            Resposta::Iniciado => None,
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
        self.modelo = None;
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
    fn le_as_respostas_do_programa() {
        assert_eq!(ler_resposta(r#"{"evento":"iniciado"}"#), Some(Resposta::Iniciado));
        assert_eq!(ler_resposta(r#"{"evento":"texto","texto":"Rode os testes."}"#), Some(Resposta::Texto { texto: "Rode os testes.".into() }));
        assert_eq!(ler_resposta(r#"{"evento":"transcrevendo","segundos":2.5}"#), Some(Resposta::Transcrevendo { segundos: 2.5 }));
        assert_eq!(ler_resposta(r#"{"evento":"saindo","motivo":"parado"}"#), Some(Resposta::Saindo { motivo: "parado".into() }));
        assert_eq!(ler_resposta("ruído do whisper.cpp"), None);
        assert_eq!(ler_resposta(r#"{"evento":"fim"}"#), None);
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
