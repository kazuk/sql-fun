use bollard::{
    Docker,
    exec::StartExecResults,
    models::ExecConfig,
    query_parameters::{CreateContainerOptions, CreateImageOptionsBuilder, StartContainerOptions},
    secret::ContainerCreateBody,
};
use futures_util::{TryStreamExt, stream::StreamExt};

#[derive(Debug, Clone)]
pub struct ContainerId(String);

impl From<&str> for ContainerId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl ContainerId {
    pub fn as_inner(&self) -> &str {
        &self.0
    }
}

/// Docker related errors
#[derive(thiserror::Error, Debug)]
pub enum DockerError {
    /// wraps bollard Error
    #[error(transparent)]
    Docker(#[from] bollard::errors::Error),
}

/// docker engine connection
#[derive(Debug)]
struct DockerClient {
    docker: Docker,
}

/// container client
#[derive(Debug)]
struct ContainerClient {
    docker: DockerClient,
    id: ContainerId,
}

impl ContainerClient {
    #[expect(dead_code)]
    pub async fn create(mut docker: DockerClient, from_image: &str) -> Result<Self, DockerError> {
        let id = docker.create_container(from_image).await?;
        Ok(Self::new(docker, id))
    }

    pub fn new(docker: DockerClient, id: ContainerId) -> Self {
        Self { docker, id }
    }

    #[expect(dead_code)]
    pub async fn start(&mut self) -> Result<(), DockerError> {
        self.docker.start_container(&self.id).await?;
        Ok(())
    }

    #[expect(dead_code)]
    pub async fn remove(mut self) -> Result<DockerClient, DockerError> {
        self.docker.remove_container(&self.id).await?;
        Ok(self.docker)
    }
}

impl DockerClient {
    #[expect(dead_code)]
    pub fn connect() -> Result<Self, DockerError> {
        let docker = Docker::connect_with_socket_defaults()?;
        Ok(Self { docker })
    }

    #[expect(dead_code)]
    pub async fn create_image(&mut self, from_image: &str) -> Result<(), DockerError> {
        let options = CreateImageOptionsBuilder::default()
            .from_image(from_image)
            .build();
        let _image_created = self
            .docker
            .create_image(Some(options), None, None)
            .try_collect::<Vec<_>>()
            .await?;

        Ok(())
    }

    pub async fn create_container(&mut self, from_image: &str) -> Result<ContainerId, DockerError> {
        let container_config = ContainerCreateBody {
            image: Some(String::from(from_image)),
            tty: Some(true),
            ..Default::default()
        };

        let id = self
            .docker
            .create_container(None::<CreateContainerOptions>, container_config)
            .await?
            .id;
        Ok(ContainerId::from(id.as_ref()))
    }

    pub async fn start_container(&mut self, id: &ContainerId) -> Result<(), DockerError> {
        self.docker
            .start_container(id.as_inner(), None::<StartContainerOptions>)
            .await?;
        Ok(())
    }

    #[expect(dead_code)]
    pub async fn exec_attached(
        &mut self,
        id: &ContainerId,
        args: &[&str],
    ) -> Result<(), DockerError> {
        let args = args.iter().map(ToString::to_string).collect();

        let config = ExecConfig {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(args),
            ..Default::default()
        };

        let exec = self.docker.create_exec(id.as_inner(), config).await?.id;

        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(&exec, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                print!("{msg}");
            }
        } else {
            unreachable!();
        }
        Ok(())
    }

    pub async fn remove_container(&mut self, id: &ContainerId) -> Result<(), DockerError> {
        self.docker
            .remove_container(
                id.as_inner(),
                Some(
                    bollard::query_parameters::RemoveContainerOptionsBuilder::default()
                        .force(true)
                        .build(),
                ),
            )
            .await?;
        Ok(())
    }
}
