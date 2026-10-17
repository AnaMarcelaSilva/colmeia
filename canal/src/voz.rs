//! A voz, do lado de quem a usa (a tela e a abelha fixa): abrir o
//! `colmeia-voz` só quando precisa, conversar com ele por uma linha de JSON
//! na entrada e na saída, e a conversa dos comandos falados: ouvir, mandar a
//! frase ao núcleo (que entende e responde), mostrar e falar a resposta e
//! confirmar o que pede "sim". O áudio nunca passa por aqui: só o texto.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use serde::Deserialize;

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
    Vigiando {
        ligado: bool,
    },
    Chamado,
    Falando,
    Falou,
    Amostra {
        numero: usize,
        #[serde(default)]
        segundos: f32,
    },
    Aprendido,
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

/// Os arquivos que o núcleo baixou e conferiu: o Whisper e, se a voz da
/// abelha estiver baixada, o Kokoro (modelo, voz e runtime).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arquivos {
    pub whisper: String,
    pub fala: Option<[String; 3]>,
}

#[derive(Deserialize, Default)]
struct EstadoModelo {
    estado: String,
    #[serde(default)]
    caminho: String,
    #[serde(default)]
    caminhos: Vec<String>,
}

fn estado(caminho: &str) -> Result<EstadoModelo, String> {
    let corpo = crate::pedir("GET", caminho).map_err(|e| format!("Não consegui falar com o núcleo: {e}"))?;
    serde_json::from_str(&corpo).map_err(|e| format!("Resposta estranha do núcleo: {e}"))
}

impl Arquivos {
    /// Pergunta ao núcleo. Sem o Whisper, não há o que abrir.
    pub fn do_nucleo() -> Result<Arquivos, String> {
        let w = estado("/v1/voz")?;
        if w.estado != "pronto" || w.caminho.is_empty() {
            return Err("O modelo de voz não está baixado. Ligue a voz em Ajustes.".into());
        }
        let fala = estado("/v1/voz/fala")
            .ok()
            .filter(|f| f.estado == "pronto" && f.caminhos.len() == 3)
            .map(|f| [f.caminhos[0].clone(), f.caminhos[1].clone(), f.caminhos[2].clone()]);
        Ok(Arquivos { whisper: w.caminho, fala })
    }
}

/// Uma resposta com o número do processo que a mandou.
pub type Marcada = (u64, Resposta);

/// O `colmeia-voz` aberto. Soltar fecha a entrada, e ele sai sozinho.
pub struct Processo {
    filho: Option<Child>,
    entrada: Option<ChildStdin>,
    pub geracao: u64,
}

impl Processo {
    /// Abre o programa; uma thread lê as respostas e as manda marcadas com
    /// `geracao`, chamando `acordar` (o pedido de redesenho de quem usa).
    pub fn abrir(arquivos: &Arquivos, geracao: u64, envio: Sender<Marcada>, acordar: Arc<dyn Fn() + Send + Sync>) -> Result<Processo, String> {
        let programa = crate::executavel("colmeia-voz", "COLMEIA_VOZ").ok_or("Não encontrei o programa colmeia-voz ao lado da Colmeia.")?;
        let mut comando = Command::new(programa);
        comando.arg("--modelo").arg(&arquivos.whisper);
        if let Some([modelo, voz, runtime]) = &arquivos.fala {
            comando.arg("--fala-modelo").arg(modelo).arg("--fala-voz").arg(voz).arg("--fala-runtime").arg(runtime);
        }
        comando.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        // No Windows, sem abrir uma janela de console.
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut comando, 0x0800_0000);
        let mut filho = comando.spawn().map_err(|e| format!("Não consegui abrir a voz: {e}"))?;
        let (Some(entrada), Some(saida)) = (filho.stdin.take(), filho.stdout.take()) else {
            let _ = filho.kill();
            let _ = filho.wait();
            return Err("Não consegui abrir a voz.".into());
        };
        std::thread::spawn(move || {
            for linha in BufReader::new(saida).lines() {
                let Ok(linha) = linha else { break };
                if let Some(r) = ler_resposta(&linha) {
                    let _ = envio.send((geracao, r));
                    acordar();
                }
            }
            let _ = envio.send((geracao, Resposta::Fim));
            acordar();
        });
        Ok(Processo { filho: Some(filho), entrada: Some(entrada), geracao })
    }

    /// Manda uma linha de pedido (JSON, sem a quebra de linha).
    pub fn mandar(&mut self, pedido: &serde_json::Value) -> Result<(), String> {
        let Some(entrada) = self.entrada.as_mut() else { return Err("A voz fechou.".into()) };
        let linha = format!("{pedido}\n");
        entrada.write_all(linha.as_bytes()).and_then(|_| entrada.flush()).map_err(|_| "A voz fechou sem responder. Tente de novo.".to_string())
    }
}

impl Drop for Processo {
    fn drop(&mut self) {
        // Sem a entrada, o programa sai; recolhe numa thread para não travar a tela.
        drop(self.entrada.take());
        if let Some(mut filho) = self.filho.take() {
            std::thread::spawn(move || {
                let _ = filho.wait();
            });
        }
    }
}

pub fn pedido(comando: &str) -> serde_json::Value {
    serde_json::json!({ "comando": comando })
}

/// O que o núcleo respondeu a uma frase (ou a um Sim clicado).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RespostaNucleo {
    pub texto: String,
    /// A ficha da pergunta que espera "sim".
    #[serde(default)]
    pub confirmar: String,
    /// A tarefa a abrir.
    #[serde(default)]
    pub abrir: i64,
}

/// Uma pergunta livre (com IA) pode levar o prazo inteiro do Claude Code no núcleo.
const ESPERA_COMANDO: std::time::Duration = std::time::Duration::from_secs(75);

fn post(caminho: &str, corpo: serde_json::Value) -> Result<RespostaNucleo, String> {
    let (status, texto) =
        crate::pedir_com_corpo_ate("POST", caminho, Some(&corpo.to_string()), ESPERA_COMANDO).map_err(|e| format!("Não consegui falar com o núcleo: {e}"))?;
    if status != 200 {
        let erro = serde_json::from_str::<serde_json::Value>(&texto).ok().and_then(|v| v["erro"].as_str().map(String::from)).unwrap_or(texto);
        return Err(erro);
    }
    serde_json::from_str(&texto).map_err(|e| format!("Resposta estranha do núcleo: {e}"))
}

pub fn comando(perfil: i64, texto: &str) -> Result<RespostaNucleo, String> {
    post(&format!("/v1/perfis/{perfil}/voz/comando"), serde_json::json!({ "texto": texto }))
}

pub fn confirmar(perfil: i64, ficha: &str, sim: bool) -> Result<RespostaNucleo, String> {
    post(&format!("/v1/perfis/{perfil}/voz/confirmar"), serde_json::json!({ "ficha": ficha, "sim": sim }))
}

/// O nome que a abelha fixa reconhece, como o núcleo vê.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NomeOuvido {
    pub nome: String,
    pub padrao: bool,
    pub arquivo: String,
    pub gravado: bool,
}

impl NomeOuvido {
    /// As referências a passar ao "vigiar": nenhuma (as do nome padrão) ou o
    /// arquivo das gravações. Sem nenhuma das duas, não dá para ouvir.
    pub fn referencias(&self) -> Option<Option<String>> {
        if self.padrao {
            Some(None)
        } else if self.gravado {
            Some(Some(self.arquivo.clone()))
        } else {
            None
        }
    }
}

pub fn nome_ouvido(perfil: i64) -> Result<NomeOuvido, String> {
    let corpo = crate::pedir("GET", &format!("/v1/perfis/{perfil}/voz/nome")).map_err(|e| format!("Não consegui falar com o núcleo: {e}"))?;
    serde_json::from_str(&corpo).map_err(|e| format!("Resposta estranha do núcleo: {e}"))
}

pub fn esquecer_nome(perfil: i64) -> Result<NomeOuvido, String> {
    let (status, corpo) =
        crate::pedir_com_corpo("DELETE", &format!("/v1/perfis/{perfil}/voz/nome"), None).map_err(|e| format!("Não consegui falar com o núcleo: {e}"))?;
    if status != 200 {
        return Err(corpo);
    }
    serde_json::from_str(&corpo).map_err(|e| format!("Resposta estranha do núcleo: {e}"))
}

/// A resposta do núcleo, de uma thread para a conversa.
type DoNucleo = Result<RespostaNucleo, String>;

/// Em que pé está a conversa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fase {
    Parada,
    /// O programa está abrindo o microfone.
    Abrindo,
    Ouvindo,
    Transcrevendo,
    /// A frase foi ao núcleo.
    Pensando,
}

/// O que quem usa faz com o que chegou.
#[derive(Clone, Debug, PartialEq)]
pub enum Saida {
    /// A resposta para o balão (e já mandada para a voz, se ligada).
    Resposta(RespostaNucleo),
    /// O que foi ouvido (para mostrar enquanto o núcleo pensa).
    Ouvido(String),
    Erro(String),
    /// Ouviu o nome: o microfone segue aberto para o comando.
    Chamado,
    /// O microfone ficou aberto ouvindo o nome (ou fechou).
    Vigiando(bool),
    Falando(bool),
    /// Uma gravação do nome guardada (na memória do programa).
    Amostra(usize),
    Aprendido,
}

/// A conversa dos comandos falados de um perfil, com um `colmeia-voz` só.
pub struct Conversa {
    pub perfil: i64,
    processo: Option<Processo>,
    respostas: (Sender<Marcada>, Receiver<Marcada>),
    nucleo: (Sender<DoNucleo>, Receiver<DoNucleo>),
    geracao: u64,
    fase: Fase,
    acordar: Arc<dyn Fn() + Send + Sync>,
    /// A resposta também é falada (a voz da abelha), com ou sem o efeito.
    pub falar: bool,
    pub efeito: bool,
    /// O que pedir de novo se o programa sair e ainda for preciso ouvir o nome.
    vigia: Option<(Option<String>, u8)>,
    vigiando: bool,
    falando: bool,
    /// A pergunta no ar (a ficha do "sim").
    pub pergunta: Option<String>,
}

impl Conversa {
    pub fn nova(perfil: i64, acordar: Arc<dyn Fn() + Send + Sync>) -> Conversa {
        Conversa {
            perfil,
            processo: None,
            respostas: mpsc::channel(),
            nucleo: mpsc::channel(),
            geracao: 0,
            fase: Fase::Parada,
            acordar,
            falar: false,
            efeito: false,
            vigia: None,
            vigiando: false,
            falando: false,
            pergunta: None,
        }
    }

    pub fn fase(&self) -> Fase {
        self.fase
    }

    /// O microfone está aberto (para o comando ou ouvindo o nome).
    pub fn microfone_aberto(&self) -> bool {
        self.vigiando || matches!(self.fase, Fase::Abrindo | Fase::Ouvindo)
    }

    pub fn vigiando(&self) -> bool {
        self.vigiando
    }

    pub fn falando(&self) -> bool {
        self.falando
    }

    fn garantir(&mut self) -> Result<&mut Processo, String> {
        if self.processo.is_none() {
            let arquivos = Arquivos::do_nucleo()?;
            self.geracao += 1;
            self.processo = Some(Processo::abrir(&arquivos, self.geracao, self.respostas.0.clone(), Arc::clone(&self.acordar))?);
        }
        Ok(self.processo.as_mut().expect("aberto acima"))
    }

    fn mandar(&mut self, pedido: serde_json::Value) -> Result<(), String> {
        let r = self.garantir()?.mandar(&pedido);
        if r.is_err() {
            self.processo = None;
        }
        r
    }

    /// Clicar para falar: abre o microfone para um comando, que fecha na pausa.
    pub fn ouvir(&mut self) -> Result<(), String> {
        if !matches!(self.fase, Fase::Parada) {
            return Ok(());
        }
        self.mandar(serde_json::json!({ "comando": "ouvir", "ate_silencio": true }))?;
        self.fase = Fase::Abrindo;
        Ok(())
    }

    /// Fecha o microfone e joga o áudio fora.
    pub fn cancelar(&mut self) {
        if matches!(self.fase, Fase::Abrindo | Fase::Ouvindo) {
            let _ = self.mandar(pedido("cancelar"));
            self.fase = Fase::Parada;
        }
    }

    /// Para a voz da abelha no meio.
    pub fn calar(&mut self) {
        if self.processo.is_some() {
            let _ = self.mandar(pedido("calar"));
        }
    }

    /// Ouvir o nome (as referências do arquivo ou as do nome padrão).
    pub fn vigiar(&mut self, referencias: Option<String>, sensibilidade: u8) -> Result<(), String> {
        self.vigia = Some((referencias.clone(), sensibilidade));
        let mut p = serde_json::json!({ "comando": "vigiar", "sensibilidade": sensibilidade });
        if let Some(r) = referencias {
            p["referencias"] = r.into();
        }
        self.mandar(p)
    }

    pub fn desvigiar(&mut self) {
        self.vigia = None;
        if self.processo.is_some() {
            let _ = self.mandar(pedido("desvigiar"));
        }
        self.vigiando = false;
    }

    /// Grava uma vez o nome (até a pausa).
    pub fn amostra(&mut self) -> Result<(), String> {
        self.mandar(pedido("amostra"))?;
        self.fase = Fase::Abrindo;
        Ok(())
    }

    pub fn aprender(&mut self, nome: &str, destino: &str) -> Result<(), String> {
        self.mandar(serde_json::json!({ "comando": "aprender", "nome": nome, "destino": destino }))
    }

    pub fn esquecer_amostras(&mut self) {
        if self.processo.is_some() {
            let _ = self.mandar(pedido("esquecer_amostras"));
        }
    }

    /// Fala um texto com a voz da abelha (se ligada).
    pub fn dizer(&mut self, texto: &str) {
        if self.falar {
            let efeito = self.efeito;
            if self.mandar(serde_json::json!({ "comando": "falar", "texto": texto, "efeito": efeito })).is_err() {
                self.falando = false;
            }
        }
    }

    /// Uma frase escrita (no resumo da abelha) vai ao núcleo como uma falada,
    /// sem abrir o microfone. Devolve `false` se já há uma conversa em andamento.
    pub fn escrever(&mut self, texto: &str) -> bool {
        let frase = texto.trim().to_string();
        if frase.is_empty() || !matches!(self.fase, Fase::Parada) {
            return false;
        }
        self.calar();
        self.pergunta = None;
        self.fase = Fase::Pensando;
        self.perguntar(move |perfil| comando(perfil, &frase));
        true
    }

    /// Responde à pergunta no ar (o Sim ou o Não clicado).
    pub fn responder(&mut self, sim: bool) {
        let Some(ficha) = self.pergunta.take() else { return };
        self.fase = Fase::Pensando;
        self.perguntar(move |perfil| confirmar(perfil, &ficha, sim));
    }

    /// Fecha o programa na hora (desligou a voz, trocou de perfil, fechou).
    pub fn fechar(&mut self) {
        self.processo = None;
        self.fase = Fase::Parada;
        self.vigia = None;
        self.vigiando = false;
        self.falando = false;
        self.pergunta = None;
    }

    /// Pede ao núcleo numa thread (a tela não espera a rede).
    fn perguntar(&mut self, pedir: impl FnOnce(i64) -> Result<RespostaNucleo, String> + Send + 'static) {
        let (envio, acordar, perfil) = (self.nucleo.0.clone(), Arc::clone(&self.acordar), self.perfil);
        std::thread::spawn(move || {
            let _ = envio.send(pedir(perfil));
            acordar();
        });
    }

    /// O que chegou do programa e do núcleo, já aplicado à conversa.
    pub fn receber(&mut self) -> Vec<Saida> {
        let mut saidas = Vec::new();
        while let Ok((geracao, r)) = self.respostas.1.try_recv() {
            if self.processo.as_ref().is_none_or(|p| p.geracao != geracao) {
                continue;
            }
            saidas.extend(self.aplicar(r));
        }
        while let Ok(r) = self.nucleo.1.try_recv() {
            self.fase = Fase::Parada;
            match r {
                Ok(resposta) => {
                    self.pergunta = (!resposta.confirmar.is_empty()).then(|| resposta.confirmar.clone());
                    self.dizer(&resposta.texto.clone());
                    saidas.push(Saida::Resposta(resposta));
                }
                Err(e) => saidas.push(Saida::Erro(e)),
            }
        }
        saidas
    }

    fn aplicar(&mut self, r: Resposta) -> Option<Saida> {
        match r {
            Resposta::Iniciado => None,
            Resposta::Ouvindo => {
                if self.fase == Fase::Abrindo {
                    self.fase = Fase::Ouvindo;
                }
                None
            }
            Resposta::Chamado => {
                self.fase = Fase::Ouvindo;
                Some(Saida::Chamado)
            }
            Resposta::Transcrevendo { .. } => {
                self.fase = Fase::Transcrevendo;
                None
            }
            Resposta::Texto { texto } => {
                if texto.trim().is_empty() {
                    self.fase = Fase::Parada;
                    return Some(Saida::Erro("Não entendi o que foi dito. Tente de novo, mais perto do microfone.".into()));
                }
                self.fase = Fase::Pensando;
                let frase = texto.clone();
                self.perguntar(move |perfil| comando(perfil, &frase));
                Some(Saida::Ouvido(texto))
            }
            Resposta::Erro { mensagem } => {
                self.fase = Fase::Parada;
                Some(Saida::Erro(mensagem))
            }
            Resposta::Vigiando { ligado } => {
                self.vigiando = ligado;
                if !ligado {
                    self.vigia = None;
                }
                Some(Saida::Vigiando(ligado))
            }
            Resposta::Falando => {
                self.falando = true;
                Some(Saida::Falando(true))
            }
            Resposta::Falou => {
                self.falando = false;
                Some(Saida::Falando(false))
            }
            Resposta::Amostra { numero, .. } => {
                self.fase = Fase::Parada;
                Some(Saida::Amostra(numero))
            }
            Resposta::Aprendido => Some(Saida::Aprendido),
            // Parado por muito tempo: sai sozinho e leva os modelos junto.
            Resposta::Saindo { .. } => {
                self.processo = None;
                None
            }
            Resposta::Fim => {
                let no_meio = !matches!(self.fase, Fase::Parada | Fase::Pensando);
                let vigia = self.vigia.take();
                self.processo = None;
                self.vigiando = false;
                self.falando = false;
                if !matches!(self.fase, Fase::Pensando) {
                    self.fase = Fase::Parada;
                }
                // Ouvindo o nome, o programa não devia sair: abre de novo uma vez.
                if let Some((r, s)) = vigia
                    && self.vigiar(r, s).is_ok()
                {
                    return None;
                }
                no_meio.then(|| Saida::Erro("A voz fechou sem terminar. Tente de novo.".into()))
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conversa() -> Conversa {
        Conversa::nova(1, Arc::new(|| {}))
    }

    #[test]
    fn le_as_respostas_do_programa() {
        assert_eq!(ler_resposta(r#"{"evento":"iniciado"}"#), Some(Resposta::Iniciado));
        assert_eq!(ler_resposta(r#"{"evento":"texto","texto":"Rodar a loja web."}"#), Some(Resposta::Texto { texto: "Rodar a loja web.".into() }));
        assert_eq!(ler_resposta(r#"{"evento":"vigiando","ligado":true}"#), Some(Resposta::Vigiando { ligado: true }));
        assert_eq!(ler_resposta(r#"{"evento":"amostra","numero":2,"segundos":0.6}"#), Some(Resposta::Amostra { numero: 2, segundos: 0.6 }));
        assert_eq!(ler_resposta(r#"{"evento":"chamado"}"#), Some(Resposta::Chamado));
        assert_eq!(ler_resposta("ruído do whisper.cpp"), None);
        assert_eq!(ler_resposta(r#"{"evento":"fim"}"#), None);
    }

    #[test]
    fn a_fase_anda_com_as_respostas() {
        let mut c = conversa();
        c.fase = Fase::Abrindo;
        assert_eq!(c.aplicar(Resposta::Ouvindo), None);
        assert_eq!(c.fase(), Fase::Ouvindo);
        assert!(c.microfone_aberto());
        assert_eq!(c.aplicar(Resposta::Transcrevendo { segundos: 1.0 }), None);
        assert_eq!(c.fase(), Fase::Transcrevendo);
        assert!(!c.microfone_aberto());
        // Texto vazio vira aviso; erro também, e a conversa para.
        assert!(matches!(c.aplicar(Resposta::Texto { texto: " ".into() }), Some(Saida::Erro(_))));
        assert_eq!(c.fase(), Fase::Parada);
        c.fase = Fase::Ouvindo;
        assert_eq!(c.aplicar(Resposta::Erro { mensagem: "Não ouvi o pedido.".into() }), Some(Saida::Erro("Não ouvi o pedido.".into())));
        assert_eq!(c.fase(), Fase::Parada);
    }

    #[test]
    fn ouvir_o_nome_abre_o_microfone_e_o_chamado_ouve_o_comando() {
        let mut c = conversa();
        assert_eq!(c.aplicar(Resposta::Vigiando { ligado: true }), Some(Saida::Vigiando(true)));
        assert!(c.microfone_aberto() && c.vigiando());
        assert_eq!(c.aplicar(Resposta::Chamado), Some(Saida::Chamado));
        assert_eq!(c.fase(), Fase::Ouvindo);
        assert_eq!(c.aplicar(Resposta::Vigiando { ligado: false }), Some(Saida::Vigiando(false)));
        assert!(!c.vigiando());
    }

    #[test]
    fn o_programa_saindo_no_meio_avisa_e_parado_nao() {
        let mut c = conversa();
        c.fase = Fase::Ouvindo;
        assert!(matches!(c.aplicar(Resposta::Fim), Some(Saida::Erro(_))));
        assert_eq!(c.fase(), Fase::Parada);
        assert_eq!(c.aplicar(Resposta::Saindo { motivo: "parado".into() }), None);
        assert_eq!(c.aplicar(Resposta::Fim), None);
    }

    #[test]
    fn a_voz_e_as_gravacoes() {
        let mut c = conversa();
        assert_eq!(c.aplicar(Resposta::Falando), Some(Saida::Falando(true)));
        assert!(c.falando());
        assert_eq!(c.aplicar(Resposta::Falou), Some(Saida::Falando(false)));
        c.fase = Fase::Ouvindo;
        assert_eq!(c.aplicar(Resposta::Amostra { numero: 3, segundos: 0.7 }), Some(Saida::Amostra(3)));
        assert_eq!(c.fase(), Fase::Parada);
        assert_eq!(c.aplicar(Resposta::Aprendido), Some(Saida::Aprendido));
    }

    #[test]
    fn escrever_so_parada_e_com_texto() {
        let mut c = conversa();
        assert!(!c.escrever("   "));
        assert_eq!(c.fase(), Fase::Parada);
        // No meio de outra conversa, a frase escrita não vai (nada chega ao núcleo).
        c.fase = Fase::Ouvindo;
        assert!(!c.escrever("o que eu fiz hoje?"));
        c.fase = Fase::Pensando;
        assert!(!c.escrever("o que eu fiz hoje?"));
    }

    #[test]
    fn referencias_do_nome() {
        let padrao = NomeOuvido { nome: "Colmeia".into(), padrao: true, arquivo: "/d/voz/nome-1.json".into(), gravado: false };
        assert_eq!(padrao.referencias(), Some(None));
        let gravado = NomeOuvido { nome: "Zuzu".into(), padrao: false, arquivo: "/d/voz/nome-1.json".into(), gravado: true };
        assert_eq!(gravado.referencias(), Some(Some("/d/voz/nome-1.json".into())));
        let sem = NomeOuvido { gravado: false, ..gravado };
        assert_eq!(sem.referencias(), None);
    }

    #[test]
    fn resposta_do_nucleo() {
        let r: RespostaNucleo = serde_json::from_str(r#"{"texto":"Parar o projeto loja-web?","confirmar":"ab12"}"#).unwrap();
        assert_eq!(r, RespostaNucleo { texto: "Parar o projeto loja-web?".into(), confirmar: "ab12".into(), abrir: 0 });
        let r: RespostaNucleo = serde_json::from_str(r#"{"texto":"Abrindo a tarefa Login.","abrir":7}"#).unwrap();
        assert_eq!(r.abrir, 7);
    }
}
