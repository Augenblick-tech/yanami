use anyhow::Result;

pub trait FromContext: Sized {
    type Ctx;
    fn build_from(ctx: &Self::Ctx) -> Result<Self>;
}

pub trait Boss: Send + Sync {
    type Ctx;
    fn context(&self) -> &Self::Ctx;

    fn get<T: FromContext<Ctx = Self::Ctx>>(&self) -> Result<T> {
        T::build_from(self.context())
    }
}
