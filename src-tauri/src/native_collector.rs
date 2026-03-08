use crate::types::{CollectorMessage, NativeError};
use crate::platform_proc::PlatformProc;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};
use crate::offsets::WuwaOffset;

/// 플랫폼별 네이티브 프로세스 래퍼
pub struct NativeCollector {
    proc: PlatformProc,
}

impl NativeCollector {
    pub async fn new(proc_name: &str) -> Result<Self> {
        let proc = PlatformProc::new(proc_name)?;
        Ok(Self { proc })
    }
}

pub async fn collection_loop(
    collector_arc: Arc<Mutex<Option<NativeCollector>>>,
    pm_tx: mpsc::Sender<CollectorMessage>,
    mut shutdown_rx: oneshot::Receiver<()>,
    offsets_arc: Arc<Mutex<Option<Vec<WuwaOffset>>>>,
) {
    let mut offset_reported = false;
    loop {
        let mut delay_ms: u64 = 120;
        // Work Phase
        {
            // 1. 상태 관리자를 잠그고 공유 상태에 접근합니다.
            let mut collector_opt_guard = collector_arc.lock().await;

            // 2. Option이 Some일 때만 로직을 수행합니다.
            //    (다른 곳에서 이미 None으로 만들었다면 루프를 종료합니다)
            let Some(collector) = &mut *collector_opt_guard else {
                log::info!("Collection loop exiting: collector is None");
                break;
            };

            let offsets_guard = offsets_arc.lock().await;

            // 3. get_location을 호출하고 결과를 매칭합니다.
            match collector.proc.get_location(&*offsets_guard).await {
                // 성공 시 데이터 전송
                Ok(loc) => {
                    if !offset_reported {
                        if let Some(name) = collector.proc.get_active_offset_name() {
                            // RtcSupervisor에게 OffsetFound 메시지를 보냅니다.
                            if pm_tx.send(CollectorMessage::OffsetFound(name)).await.is_err() {
                                log::info!("Collection loop exiting: no receiver");
                                break;
                            }
                            offset_reported = true; // 보고 완료로 표시
                        }
                    }
                    if pm_tx.send(CollectorMessage::Data(loc)).await.is_err() {
                        log::info!("Collection loop exiting: no receiver");
                        break;
                    }
                    delay_ms = 80;
                }

                // '프로세스 종료'는 치명적 오류
                Err(NativeError::ProcessTerminated) => {
                    log::info!("Collection loop exiting: process is terminated");
                    let _ = pm_tx.send(CollectorMessage::Terminated).await;
                    break;
                }

                // 그 외 모든 오류는 일시적인 것으로 간주
                Err(e) => {
                    let msg = e.to_string();
                    let is_transient_gworld = msg.contains("'GWorld' 포인터가 유효하지 않습니다. raw=0")
                        || msg.contains("'GWorld' 위치");
                    if is_transient_gworld {
                        // 로딩/씬 전환 중 일시 상태는 빠르게 재시도하고 UI 이벤트는 생략합니다.
                        delay_ms = 40;
                    } else {
                        if pm_tx
                            .send(CollectorMessage::TemporalError(msg))
                            .await
                            .is_err()
                        {
                            log::info!("Collection loop exiting: no receiver");
                            break;
                        }
                    }
                }
            }
        }
        // Sleep Phase
        tokio::select! {
            // 외부(PeerManager)로부터의 종료 신호 처리
            _ = &mut shutdown_rx => {
                log::info!("Collection loop exiting: exit signal received");
                break;
            }

            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
        }
    }
}
