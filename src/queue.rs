use crate::vertspec::Progdef;

/// ensure all required job queues exists
pub async fn ensure_queues(broker: &lapin::Connection, verts: &[Progdef]) -> anyhow::Result<()> {
    let channel = broker.create_channel().await?;
    for progdef in verts {
        channel
            .queue_declare(
                &progdef.hash.job_mailbox(),
                lapin::options::QueueDeclareOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await?;
    }
    Ok(())
}
