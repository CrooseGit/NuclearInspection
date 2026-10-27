FROM tiryoh/ros-desktop-vnc:noetic

ENV DEBIAN_FRONTEND=noninteractive

# Install essential tools in one step and clean up to save space
RUN apt update && apt install -y \
    build-essential \
    libc6-dev \
    python3-dev \
    openjdk-17-jdk \
    openjdk-11-jdk \
    lsb-release \
    gnupg \
    ant \
    wget \
    unzip \
    git \
    ros-noetic-rosbridge-server \
    ros-noetic-navigation \
    ros-noetic-jackal-* \
    ros-noetic-teleop-twist-keyboard \
    ros-noetic-pcl-ros \
    ros-noetic-velodyne \
    ros-noetic-velocity-controllers \
    ros-noetic-teb-local-planner \
    ros-noetic-pointcloud-to-laserscan \
    ros-noetic-gazebo-ros-pkgs \
    ros-noetic-velodyne-gazebo-plugins \
    ros-noetic-joint-state-publisher-gui \ 
    ros-noetic-ros-controllers \
    ros-noetic-openslam-gmapping \
    && apt clean && rm -rf /var/lib/apt/lists/*

## Install Apache Maven
RUN wget -qO- https://dlcdn.apache.org/maven/maven-3/3.9.16/binaries/apache-maven-3.9.16-bin.tar.gz | tar xz -C /opt/ \
    && ln -s /opt/apache-maven-3.9.9 /opt/maven
ENV M2_HOME='/opt/maven'
ENV PATH="$M2_HOME/bin:$PATH"



# Create the ubuntu user if it does not exist
RUN id ubuntu || ( \
    useradd -m -s /bin/bash ubuntu && \
    echo "ubuntu:ubuntu" | chpasswd \
)

# Create the Desktop directory and set proper ownership
RUN mkdir -p /home/ubuntu/Desktop && chown -R ubuntu:ubuntu /home/ubuntu


## Seem to need to set the locale to get mvn and ant to recoginse some characters
USER root
RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && \
    locale-gen
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US.en
ENV LC_ALL=en_US.UTF-8

## Switch to the ubuntu user for subsequent build steps
USER ubuntu
WORKDIR /home/ubuntu

# Install Gazebo and required ROS packages
USER root
RUN sh -c 'echo "deb http://packages.osrfoundation.org/gazebo/ubuntu-stable `lsb_release -cs` main" > /etc/apt/sources.list.d/gazebo-stable.list' && \
    curl -sSL https://packages.osrfoundation.org/gazebo.key | apt-key add - && \
    apt update && apt install -y ros-noetic-gazebo-ros && \
    apt clean && rm -rf /var/lib/apt/lists/*

## Install Rust and build cargo project
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="$HOME/.cargo/bin:$PATH"
RUN . "$HOME/.cargo/env"
WORKDIR /home/ubuntu/rust_nodes

## Install IntelliJ IDEA (Community Edition)
WORKDIR /tmp
RUN wget https://download.jetbrains.com/idea/ideaIC-2024.3.3-aarch64.tar.gz
RUN tar -xzf ideaIC-*.tar.gz -C /opt


RUN ln -s /opt/idea-IC-*/bin/idea.sh /usr/local/bin/intellij

## Cleanup
RUN rm -rf /tmp/ideaIC-2024.3.3.tar.gz

#FOR MCAPL
RUN mkdir -p ~/.jpf && echo "mcapl = /home/ubuntu/mcapl" > ~/.jpf/site.properties
ENV AJPF_HOME=/home/ubuntu/mcapl
ENV CLASSPATH=/home/ubuntu/mcapl/bin

COPY ./entrypoint.sh /
ENTRYPOINT [ "/bin/bash", "-c", "/entrypoint.sh" ]


    


    
